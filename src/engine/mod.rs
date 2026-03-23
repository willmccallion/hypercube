//! Simulation engine: builds the hypercube, injects traffic, and runs the
//! tick-synchronized simulation to completion.
//!
//! The core [`Engine`] is synchronous — a single [`Engine::tick`] call advances
//! all nodes one step using double-buffered link queues. When the `async`
//! feature is enabled, [`AsyncEngine`] provides the original Tokio-based
//! execution model where each node runs as an independent task.

mod builder;

#[cfg(feature = "async")]
mod async_engine;

pub use builder::SimulationConfig;

#[cfg(feature = "async")]
pub use async_engine::AsyncEngine;

use std::collections::VecDeque;
use std::sync::Arc;

use rand::RngCore;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand_chacha::ChaCha8Rng;

use crate::packet::{Packet, Phase};
use crate::routing::RoutingStrategy;
use crate::stats::{NodeStats, SimulationStats};
use crate::topology::{FaultSet, Hypercube, NodeId};
use crate::traffic::TrafficPattern;

/// Double-buffered link queue for one directed edge.
///
/// Packets written during the current tick land in `write`. At the end of
/// each tick, `write` is swapped into `read`, making those packets available
/// to the destination node on the *next* tick. This prevents a packet from
/// advancing more than one hop per tick.
struct LinkBuffer {
    /// Packets deposited last tick — available for reading this tick.
    read: VecDeque<Packet>,
    /// Packets being written this tick — not yet visible to destinations.
    write: VecDeque<Packet>,
}

impl LinkBuffer {
    const fn new() -> Self {
        Self {
            read: VecDeque::new(),
            write: VecDeque::new(),
        }
    }

    /// Swaps write into read for the next tick: clears read (should already
    /// be empty after consumption), then swaps the two buffers.
    fn swap(&mut self) {
        self.read.clear();
        std::mem::swap(&mut self.read, &mut self.write);
    }
}

/// Per-node mutable state tracked by the engine during simulation.
struct NodeState {
    /// Node's local RNG for routing decisions.
    rng: SmallRng,
    /// Per-dimension link load counters.
    per_link_load: Vec<u64>,
    /// Per-dimension maximum queue depth observed.
    max_queue_depth: Vec<u32>,
    /// Total packets forwarded through this node.
    packets_forwarded: u64,
    /// Packets that reached their final destination at this node.
    packets_delivered: u64,
    /// Delivery latencies (in ticks).
    delivery_latencies: Vec<u64>,
    /// Hop counts for delivered packets.
    delivery_hops: Vec<u32>,
}

impl NodeState {
    fn new(dim: usize, rng_seed: u64) -> Self {
        Self {
            rng: SmallRng::seed_from_u64(rng_seed),
            per_link_load: vec![0; dim],
            max_queue_depth: vec![0; dim],
            packets_forwarded: 0,
            packets_delivered: 0,
            delivery_latencies: Vec::new(),
            delivery_hops: Vec::new(),
        }
    }

    fn into_node_stats(self, id: NodeId) -> NodeStats {
        NodeStats {
            id,
            packets_forwarded: self.packets_forwarded,
            packets_delivered: self.packets_delivered,
            per_link_load: self.per_link_load,
            max_queue_depth: self.max_queue_depth,
            delivery_latencies: self.delivery_latencies,
            delivery_hops: self.delivery_hops,
        }
    }
}

/// A synchronous simulation engine for hypercube routing.
///
/// Created by [`Engine::build`]. Call [`Engine::inject`] to add traffic,
/// then [`Engine::run`] (or repeated [`Engine::tick`] calls) to execute.
///
/// This is the primary engine for embedding in other simulators (e.g., as a
/// network-on-chip transport layer). For standalone benchmarks with per-node async tasks,
/// see [`AsyncEngine`] (requires the `async` feature).
pub struct Engine {
    /// The hypercube topology.
    cube: Hypercube,
    /// The routing strategy.
    strategy: Arc<dyn RoutingStrategy>,
    /// Fault configuration.
    faults: FaultSet,
    /// Per-node state, indexed by node ID.
    nodes: Vec<NodeState>,
    /// Double-buffered link queues. `links[src][dim]` holds packets sent
    /// from `src` along dimension `dim`.
    links: Vec<Vec<LinkBuffer>>,
    /// Injection queues, one per node.
    inject: Vec<VecDeque<Packet>>,
    /// Current tick counter.
    current_tick: u64,
    /// Total packets injected (for stats).
    total_injected: u64,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("node_count", &self.nodes.len())
            .field("dimension", &self.cube.dim())
            .field("strategy", &self.strategy.name())
            .field("current_tick", &self.current_tick)
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Builds a new synchronous simulation engine from the given configuration.
    pub fn build(config: SimulationConfig) -> Self {
        builder::build_sync(config)
    }

    /// Injects packets from a traffic pattern into the simulation.
    ///
    /// For Valiant routing, each packet is assigned a random intermediate node.
    pub fn inject(
        &mut self,
        pattern: &dyn TrafficPattern,
        rng: &mut dyn RngCore,
    ) {
        let mut packets = pattern.generate(&self.cube, rng);

        // If the strategy is Valiant, assign random intermediates.
        let is_valiant = self.strategy.name() == "valiant";
        if is_valiant {
            let mut intermediate_rng = ChaCha8Rng::seed_from_u64(rng.next_u64());
            let node_count = self.cube.node_count();
            for packet in &mut packets {
                let intermediate = NodeId::new((intermediate_rng.next_u32() % node_count) as u16);
                packet.phase = Phase::ValiantPhase1 { intermediate };
            }
        }

        for mut packet in packets {
            packet.inject_tick = 0;
            let src_idx = packet.source.raw() as usize;
            self.inject[src_idx].push_back(packet);
            self.total_injected += 1;
        }
    }

    /// Advances the simulation by one tick.
    ///
    /// All nodes drain their incoming link buffers and injection queues,
    /// route each packet, then write outgoing packets to the appropriate
    /// link buffers. A final swap makes this tick's writes readable next tick.
    pub fn tick(&mut self) {
        let dim = self.cube.dim() as usize;
        let node_count = self.nodes.len();
        let tick = self.current_tick;

        // Phase 1: Collect incoming packets for each node.
        //
        // Node `i` reads from `links[neighbor][d].read` for each dimension `d`,
        // where `neighbor = i ^ (1 << d)`. We also drain injection queues.
        let mut incoming: Vec<Vec<Packet>> = (0..node_count).map(|_| Vec::new()).collect();

        for (node_idx, (buf, inject_q)) in incoming
            .iter_mut()
            .zip(self.inject.iter_mut())
            .enumerate()
        {
            // Drain injection queue.
            buf.extend(inject_q.drain(..));

            // Drain incoming link buffers (packets sent by neighbors last tick).
            for d in 0..dim {
                let neighbor_idx = node_idx ^ (1 << d);
                buf.extend(self.links[neighbor_idx][d].read.drain(..));
            }
        }

        // Phase 2: Route packets at each node.
        for (node_idx, buf) in incoming.iter_mut().enumerate() {
            let current = NodeId::new(node_idx as u16);
            let packets = std::mem::take(buf);

            for mut packet in packets {
                let next = self.strategy.next_hop(
                    &mut packet,
                    current,
                    &self.cube,
                    &self.faults,
                    &mut self.nodes[node_idx].rng,
                );

                match next {
                    None => {
                        // Packet arrived at destination.
                        self.nodes[node_idx].packets_delivered += 1;
                        let latency = tick.saturating_sub(packet.inject_tick);
                        self.nodes[node_idx].delivery_latencies.push(latency);
                        self.nodes[node_idx].delivery_hops.push(packet.hops);
                    }
                    Some(next_node) => {
                        let diff = current.differing_bits(next_node);
                        let dim_idx = diff.trailing_zeros() as usize;

                        packet.hops += 1;
                        self.nodes[node_idx].packets_forwarded += 1;
                        self.nodes[node_idx].per_link_load[dim_idx] += 1;

                        // Write to our own link buffer for this dimension.
                        self.links[node_idx][dim_idx].write.push_back(packet);
                    }
                }
            }
        }

        // Phase 3: Track max queue depth, then swap buffers.
        for node_idx in 0..node_count {
            for d in 0..dim {
                let depth = self.links[node_idx][d].write.len() as u32;
                let max = &mut self.nodes[node_idx].max_queue_depth[d];
                if depth > *max {
                    *max = depth;
                }
            }
        }

        for src_links in &mut self.links {
            for buf in src_links {
                buf.swap();
            }
        }

        self.current_tick += 1;
    }

    /// Runs the simulation for `max_ticks` ticks and returns aggregated stats.
    pub fn run(
        mut self,
        traffic_name: &str,
        max_ticks: u64,
    ) -> SimulationStats {
        let dim = self.cube.dim();
        let strategy_name = self.strategy.name().to_owned();
        let total_packets = self.total_injected;

        for _ in 0..max_ticks {
            self.tick();
        }

        let node_count = self.nodes.len();
        let per_node: Vec<NodeStats> = self
            .nodes
            .into_iter()
            .enumerate()
            .map(|(i, state)| state.into_node_stats(NodeId::new(i as u16)))
            .collect();

        debug_assert_eq!(per_node.len(), node_count);

        SimulationStats::aggregate(
            dim,
            &strategy_name,
            traffic_name,
            total_packets,
            per_node,
        )
    }
}
