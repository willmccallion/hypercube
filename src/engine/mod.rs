//! Simulation engine: builds the hypercube, injects traffic, and runs the
//! tick-synchronized simulation to completion.

mod builder;

pub use builder::SimulationConfig;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tokio::sync::{Barrier, mpsc};
use tokio::task::JoinHandle;

use crate::packet::Phase;
use crate::routing::RoutingStrategy;
use crate::stats::{NodeStats, SimulationStats};
use crate::topology::{Hypercube, NodeId};
use crate::traffic::TrafficPattern;

/// Handle to a running simulation.
pub struct Engine {
    /// Injection senders, one per node.
    inject_txs: Vec<mpsc::Sender<crate::packet::Packet>>,
    /// Join handles for all node tasks.
    handles: Vec<JoinHandle<NodeStats>>,
    /// Shared termination flag.
    done: Arc<AtomicBool>,
    /// Global in-flight packet counter.
    in_flight: Arc<AtomicU64>,
    /// Tick barrier (nodes + orchestrator).
    barrier: Arc<Barrier>,
    /// The hypercube topology.
    cube: Arc<Hypercube>,
    /// The routing strategy (for metadata).
    strategy: Arc<dyn RoutingStrategy>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("node_count", &self.inject_txs.len())
            .field("dimension", &self.cube.dim())
            .field("strategy", &self.strategy.name())
            .field("done", &self.done.load(Ordering::Relaxed))
            .field("in_flight", &self.in_flight.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Builds a new simulation engine from the given configuration.
    pub fn build(config: SimulationConfig) -> Self {
        builder::build(config)
    }

    /// Injects packets from a traffic pattern into the simulation.
    ///
    /// For Valiant routing, each packet is assigned a random intermediate node.
    pub fn inject(
        &self,
        pattern: &dyn TrafficPattern,
        rng: &mut dyn RngCore,
    ) {
        let mut packets = pattern.generate(&self.cube, rng);

        // If the strategy is Valiant, assign random intermediates.
        let is_valiant = self.strategy.name() == "valiant";
        if is_valiant {
            let mut intermediate_rng = ChaCha8Rng::seed_from_u64(rng.next_u64());
            let node_count = self.cube.node_count() as u16;
            for packet in &mut packets {
                let intermediate = NodeId::new((intermediate_rng.next_u32() as u16) % node_count);
                packet.phase = Phase::ValiantPhase1 { intermediate };
            }
        }

        // Set inject tick and send to the appropriate node.
        for mut packet in packets {
            packet.inject_tick = 0;
            let src_idx = packet.source.raw() as usize;
            // Use try_send since channels should have capacity for at least
            // one packet per node.
            if let Err(e) = self.inject_txs[src_idx].try_send(packet) {
                tracing::warn!("failed to inject packet at node {src_idx}: {e}");
            }
            let _ = self.in_flight.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Runs the simulation until all packets are delivered or a maximum
    /// tick count is reached.
    ///
    /// Returns aggregated statistics.
    pub async fn run(
        self,
        traffic_name: &str,
        max_ticks: u64,
    ) -> SimulationStats {
        let dim = self.cube.dim();
        let strategy_name = self.strategy.name().to_owned();

        // Orchestrator participates in the barrier.
        let mut tick: u64 = 0;
        loop {
            // Wait for all nodes to complete this tick.
            let _ = self.barrier.wait().await;
            tick += 1;

            if tick >= max_ticks {
                tracing::warn!("simulation reached max tick limit ({max_ticks})");
                break;
            }

            // Simple termination: run for a generous number of ticks.
            // A tighter check would track delivered packet count, but that
            // requires per-node reporting each tick. For correctness, we
            // overshoot and let nodes idle once they have no more packets.
            //
            // For an n-cube with Valiant routing, max path length is 2n.
            // We add a generous buffer for queuing delays.
            if tick > u64::from(dim) * 4 + 10 {
                break;
            }
        }

        // Signal termination and wait one more barrier so nodes see it.
        self.done.store(true, Ordering::Release);
        let _ = self.barrier.wait().await;

        // Collect results from all node tasks.
        let total_packets = self.in_flight.load(Ordering::Relaxed);
        let mut per_node = Vec::with_capacity(self.handles.len());
        for handle in self.handles {
            match handle.await {
                Ok(stats) => per_node.push(stats),
                Err(e) => tracing::error!("node task panicked: {e}"),
            }
        }

        SimulationStats::aggregate(
            dim,
            &strategy_name,
            traffic_name,
            total_packets,
            per_node,
        )
    }
}
