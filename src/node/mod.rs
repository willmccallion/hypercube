//! Async node task: the event loop that runs on each hypercube vertex.

use std::sync::Arc;

use rand::rngs::SmallRng;
use rand::SeedableRng;
use tokio::sync::{Barrier, mpsc};

use crate::packet::Packet;
use crate::routing::RoutingStrategy;
use crate::stats::NodeStats;
use crate::topology::{FaultSet, Hypercube, NodeId};

/// A node in the hypercube, running as an independent async task.
///
/// Each node has one `mpsc` channel per dimension for receiving packets from
/// neighbors, plus an injection channel for packets originating here. Outgoing
/// packets are sent via the corresponding neighbor's receiver channel.
pub struct Node {
    /// This node's identifier.
    id: NodeId,
    /// The hypercube topology descriptor.
    cube: Arc<Hypercube>,
    /// Per-dimension receivers: packets arriving from neighbor along dim `i`.
    rx: Vec<mpsc::Receiver<Packet>>,
    /// Per-dimension senders: for forwarding packets to neighbor along dim `i`.
    tx: Vec<mpsc::Sender<Packet>>,
    /// Receiver for packets injected at this node (originating here).
    inject_rx: mpsc::Receiver<Packet>,
    /// Shared routing strategy.
    strategy: Arc<dyn RoutingStrategy>,
    /// Shared fault configuration.
    faults: Arc<FaultSet>,
    /// Tick barrier for synchronized stepping.
    barrier: Arc<Barrier>,
    /// Total number of ticks to simulate.
    max_ticks: u64,
    /// Seed for this node's local RNG.
    rng_seed: u64,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("id", &self.id)
            .field("dimension", &self.cube.dim())
            .field("strategy", &self.strategy.name())
            .field("max_ticks", &self.max_ticks)
            .field("rng_seed", &self.rng_seed)
            .finish_non_exhaustive()
    }
}

impl Node {
    /// Creates a new node. Called by the engine builder.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: NodeId,
        cube: Arc<Hypercube>,
        rx: Vec<mpsc::Receiver<Packet>>,
        tx: Vec<mpsc::Sender<Packet>>,
        inject_rx: mpsc::Receiver<Packet>,
        strategy: Arc<dyn RoutingStrategy>,
        faults: Arc<FaultSet>,
        barrier: Arc<Barrier>,
        max_ticks: u64,
        rng_seed: u64,
    ) -> Self {
        Self {
            id,
            cube,
            rx,
            tx,
            inject_rx,
            strategy,
            faults,
            barrier,
            max_ticks,
            rng_seed,
        }
    }

    /// Runs the node's event loop for exactly `max_ticks` ticks.
    ///
    /// Returns the accumulated [`NodeStats`] for this node.
    pub async fn run(mut self) -> NodeStats {
        let dim = self.cube.dim() as usize;
        let mut rng = SmallRng::seed_from_u64(self.rng_seed);
        let mut per_link_load = vec![0u64; dim];
        let mut max_queue_depth = vec![0u32; dim];
        let mut packets_forwarded: u64 = 0;
        let mut packets_delivered: u64 = 0;
        let mut delivery_latencies = Vec::new();

        // Per-dimension overflow queues for backpressure handling.
        let mut overflow: Vec<Vec<Packet>> = vec![Vec::new(); dim];
        // Buffer for packets received this tick.
        let mut input_buffer = Vec::new();

        for tick in 0..self.max_ticks {
            // Phase 1: Drain all input channels (non-blocking).
            for rx in &mut self.rx {
                while let Ok(pkt) = rx.try_recv() {
                    input_buffer.push(pkt);
                }
            }
            // Drain injection queue.
            while let Ok(pkt) = self.inject_rx.try_recv() {
                input_buffer.push(pkt);
            }

            // Phase 2: Route each packet.
            for mut packet in std::mem::take(&mut input_buffer) {
                match self.strategy.next_hop(
                    &mut packet,
                    self.id,
                    &self.cube,
                    &self.faults,
                    &mut rng,
                ) {
                    None => {
                        // Packet arrived at destination.
                        packets_delivered += 1;
                        let latency = tick.saturating_sub(packet.inject_tick);
                        delivery_latencies.push(latency);
                    }
                    Some(next) => {
                        // Determine which dimension this neighbor is on.
                        let diff = self.id.differing_bits(next);
                        let dim_idx = diff.trailing_zeros() as usize;

                        packet.hops += 1;
                        packets_forwarded += 1;
                        per_link_load[dim_idx] += 1;
                        overflow[dim_idx].push(packet);
                    }
                }
            }

            // Phase 3: Flush overflow queues via try_send.
            for (dim_idx, queue) in overflow.iter_mut().enumerate() {
                let mut retry = Vec::new();
                for packet in queue.drain(..) {
                    if let Err(mpsc::error::TrySendError::Full(pkt)) =
                        self.tx[dim_idx].try_send(packet)
                    {
                        retry.push(pkt);
                    }
                }
                let depth = retry.len() as u32;
                if depth > max_queue_depth[dim_idx] {
                    max_queue_depth[dim_idx] = depth;
                }
                *queue = retry;
            }

            // Phase 4: Barrier — synchronize with all other nodes.
            let _ = self.barrier.wait().await;
        }

        NodeStats {
            id: self.id,
            packets_forwarded,
            packets_delivered,
            per_link_load,
            max_queue_depth,
            delivery_latencies,
        }
    }
}
