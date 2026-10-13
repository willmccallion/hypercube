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

pub use builder::{DEFAULT_CHANNEL_CAPACITY, SimulationConfig};

#[cfg(feature = "async")]
pub use async_engine::AsyncEngine;

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;

use rand::RngCore;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use rand_chacha::ChaCha8Rng;

use crate::packet::{Packet, PacketHeader, Phase};
use crate::routing::RoutingStrategy;
use crate::stats::{NodeStats, SimulationStats};
use crate::topology::{FaultSet, Hypercube, NodeId};
use crate::traffic::TrafficPattern;

/// A routed packet waiting at a node because its output queue was full.
struct Blocked<T> {
    packet: Packet<T>,
    dim: usize,
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
/// Generic over payload type `T`. The default `Engine` (i.e., `Engine<()>`)
/// is used for standalone routing experiments with [`TrafficPattern`]s.
/// Embed as `Engine<YourMessage>` to carry application payloads through
/// the network (e.g., cache coherence messages in a many-core simulator).
///
/// Created by [`Engine::build`]. Call [`Engine::inject_one`] (or
/// [`Engine::inject`] for `Engine<()>`) to add traffic, then advance
/// with [`Engine::tick`] and collect results with [`Engine::drain_delivered`].
pub struct Engine<T = ()> {
    /// The hypercube topology.
    cube: Hypercube,
    /// The routing strategy.
    strategy: Arc<dyn RoutingStrategy>,
    /// Fault configuration.
    faults: FaultSet,
    /// Per-node state, indexed by node ID.
    nodes: Vec<NodeState>,
    /// Output queues. `out_queues[src][dim]` holds packets waiting to cross
    /// the link from `src` along `dim`; each link sends one packet per tick.
    out_queues: Vec<Vec<VecDeque<Packet<T>>>>,
    /// Maximum length of each output queue.
    channel_capacity: NonZeroUsize,
    /// Packets that crossed a link last tick, indexed by receiving node.
    arriving: Vec<Vec<Packet<T>>>,
    /// Routed packets whose output queue was full, indexed by node.
    blocked: Vec<Vec<Blocked<T>>>,
    /// Injection queues, one per node.
    inject: Vec<VecDeque<Packet<T>>>,
    /// Packets delivered during the most recent tick(s), waiting to be drained.
    delivered: Vec<Packet<T>>,
    /// Current tick counter.
    current_tick: u64,
    /// Total packets injected (for stats and in-flight tracking).
    total_injected: u64,
    /// Total packets delivered so far.
    total_delivered: u64,
    /// Next packet ID for [`inject_one`](Engine::inject_one).
    next_packet_id: u64,
}

impl<T> std::fmt::Debug for Engine<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("node_count", &self.nodes.len())
            .field("dimension", &self.cube.dim())
            .field("strategy", &self.strategy.name())
            .field("current_tick", &self.current_tick)
            .field("in_flight", &(self.total_injected - self.total_delivered))
            .finish_non_exhaustive()
    }
}

// --- Methods available for any payload type T ---

impl<T: Send + 'static> Engine<T> {
    /// Builds a new synchronous simulation engine from the given configuration.
    pub fn build(config: SimulationConfig) -> Self {
        builder::build_sync(config)
    }

    /// Injects a single packet with the given source, destination, and payload.
    ///
    /// Returns the assigned packet ID, which will appear in the delivered
    /// [`Packet`] for correlation.
    ///
    /// The packet's phase is set based on the engine's routing strategy:
    /// Valiant routing assigns a random intermediate node; all other
    /// strategies use direct routing.
    pub fn inject_one(&mut self, src: NodeId, dst: NodeId, payload: T) -> u64 {
        let id = self.next_packet_id;
        self.next_packet_id += 1;

        let phase = if self.strategy.name() == "valiant" {
            let node_count = self.cube.node_count();
            let src_idx = src.raw() as usize;
            let intermediate_raw = self.nodes[src_idx].rng.next_u32() % node_count;
            Phase::ValiantPhase1 {
                intermediate: NodeId::new(intermediate_raw as u16),
            }
        } else {
            Phase::Direct
        };

        let packet = Packet {
            header: PacketHeader {
                id,
                source: src,
                destination: dst,
                phase,
                hops: 0,
                inject_tick: self.current_tick,
            },
            payload,
        };

        let src_idx = src.raw() as usize;
        self.inject[src_idx].push_back(packet);
        self.total_injected += 1;
        id
    }

    /// Advances the simulation by one tick.
    ///
    /// Every node routes the packets that arrived last tick, plus new
    /// injections, onto its output queues; packets whose queue is full wait
    /// at the node. Then every link sends the packet at the head of its queue,
    /// which reaches the neighbor next tick.
    ///
    /// Delivered packets are accumulated and retrievable via
    /// [`drain_delivered`](Engine::drain_delivered).
    pub fn tick(&mut self) {
        let dim = self.cube.dim() as usize;
        let node_count = self.nodes.len();

        for node_idx in 0..node_count {
            for blocked in std::mem::take(&mut self.blocked[node_idx]) {
                self.enqueue(node_idx, blocked.packet, blocked.dim);
            }
            let arrivals = std::mem::take(&mut self.arriving[node_idx]);
            let injected = std::mem::take(&mut self.inject[node_idx]);
            for packet in arrivals.into_iter().chain(injected) {
                self.route(node_idx, packet);
            }
        }

        for node_idx in 0..node_count {
            for d in 0..dim {
                let depth = self.out_queues[node_idx][d].len() as u32;
                let max = &mut self.nodes[node_idx].max_queue_depth[d];
                *max = (*max).max(depth);

                if let Some(mut packet) = self.out_queues[node_idx][d].pop_front() {
                    packet.hops += 1;
                    self.nodes[node_idx].packets_forwarded += 1;
                    self.nodes[node_idx].per_link_load[d] += 1;
                    self.arriving[node_idx ^ (1 << d)].push(packet);
                }
            }
        }

        self.current_tick += 1;
    }

    /// Delivers `packet` if it has arrived, otherwise queues it on the link
    /// its routing strategy picks.
    fn route(&mut self, node_idx: usize, mut packet: Packet<T>) {
        let current = NodeId::new(node_idx as u16);
        let next = self.strategy.next_hop(
            &mut packet.header,
            current,
            &self.cube,
            &self.faults,
            &mut self.nodes[node_idx].rng,
        );

        let Some(next_node) = next else {
            let latency = self.current_tick.saturating_sub(packet.inject_tick);
            let node = &mut self.nodes[node_idx];
            node.packets_delivered += 1;
            node.delivery_latencies.push(latency);
            node.delivery_hops.push(packet.hops);
            self.total_delivered += 1;
            self.delivered.push(packet);
            return;
        };

        let dim = current.differing_bits(next_node).trailing_zeros() as usize;
        self.enqueue(node_idx, packet, dim);
    }

    fn enqueue(&mut self, node_idx: usize, packet: Packet<T>, dim: usize) {
        let queue = &mut self.out_queues[node_idx][dim];
        if queue.len() < self.channel_capacity.get() {
            queue.push_back(packet);
        } else {
            self.blocked[node_idx].push(Blocked { packet, dim });
        }
    }

    /// Returns packets delivered since the last call to `drain_delivered`.
    ///
    /// Each delivered [`Packet<T>`] carries its original payload intact,
    /// along with routing metadata (source, destination, hops, latency).
    pub fn drain_delivered(&mut self) -> Vec<Packet<T>> {
        std::mem::take(&mut self.delivered)
    }

    /// Returns the current tick count.
    pub const fn tick_count(&self) -> u64 {
        self.current_tick
    }

    /// Returns the number of packets currently in the network (injected but
    /// not yet delivered).
    ///
    /// When this reaches zero, all injected packets have been delivered.
    pub const fn in_flight(&self) -> u64 {
        self.total_injected - self.total_delivered
    }

    /// Returns the total number of packets injected so far.
    pub const fn total_injected(&self) -> u64 {
        self.total_injected
    }

    /// Returns the total number of packets delivered so far.
    pub const fn total_delivered(&self) -> u64 {
        self.total_delivered
    }

    /// Returns a reference to the hypercube topology.
    pub const fn cube(&self) -> &Hypercube {
        &self.cube
    }
}

// --- Methods only available for the default payload (standalone experiments) ---

impl Engine<()> {
    /// Injects packets from a traffic pattern into the simulation.
    ///
    /// For Valiant routing, each packet is assigned a random intermediate node.
    ///
    /// This method is only available on `Engine<()>` since [`TrafficPattern`]
    /// generates unit-payload packets for routing experiments.
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
                packet.header.phase = Phase::ValiantPhase1 { intermediate };
            }
        }

        for mut packet in packets {
            packet.header.inject_tick = 0;
            let src_idx = packet.source.raw() as usize;
            self.inject[src_idx].push_back(packet);
            self.total_injected += 1;
        }
    }

    /// Runs until every packet is delivered or `max_ticks` ticks have passed,
    /// and returns aggregated stats.
    ///
    /// This is a convenience method for standalone experiments. For embedded
    /// use, call [`tick`](Engine::tick) and [`drain_delivered`](Engine::drain_delivered)
    /// in your own loop.
    pub fn run(
        mut self,
        traffic_name: &str,
        max_ticks: u64,
    ) -> SimulationStats {
        let dim = self.cube.dim();
        let strategy_name = self.strategy.name().to_owned();
        let total_packets = self.total_injected;

        while self.in_flight() > 0 && self.current_tick < max_ticks {
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
