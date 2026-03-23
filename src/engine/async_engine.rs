//! Async simulation engine: spawns per-node Tokio tasks with barrier sync.
//!
//! This is the original execution model where each hypercube node runs as an
//! independent async task communicating via `tokio::sync::mpsc` channels. It
//! is gated behind the `async` feature and preserved for standalone benchmarks
//! and large-dimension runs where per-node task parallelism is beneficial.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rand::RngCore;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tokio::sync::mpsc;

use crate::packet::{Packet, Phase};
use crate::routing::RoutingStrategy;
use crate::stats::SimulationStats;
use crate::topology::{FaultSet, Hypercube, NodeId};
use crate::traffic::TrafficPattern;

use super::SimulationConfig;

/// Pre-assembled channel endpoints for a single node (not yet spawned).
pub struct AsyncNodeConfig {
    /// Node identifier.
    pub id: NodeId,
    /// Per-dimension receivers.
    pub rx: Vec<mpsc::Receiver<Packet>>,
    /// Per-dimension senders.
    pub tx: Vec<mpsc::Sender<Packet>>,
    /// Injection receiver.
    pub inject_rx: mpsc::Receiver<Packet>,
    /// RNG seed for this node.
    pub rng_seed: u64,
}

/// An async simulation engine that spawns per-node Tokio tasks.
///
/// Each node runs as an independent `tokio::spawn` task, synchronized via a
/// shared barrier each tick. This models the distributed nature of the system
/// and can exploit CPU parallelism for large hypercubes.
///
/// For embedding in other simulators or single-threaded use, prefer
/// [`super::Engine`] (the synchronous engine).
pub struct AsyncEngine {
    /// Pre-assembled per-node state, consumed when spawning tasks.
    pub(crate) node_configs: Vec<AsyncNodeConfig>,
    /// Injection senders, one per node.
    pub(crate) inject_txs: Vec<mpsc::Sender<Packet>>,
    /// Global in-flight packet counter.
    pub(crate) in_flight: Arc<AtomicU64>,
    /// The hypercube topology.
    pub(crate) cube: Arc<Hypercube>,
    /// The routing strategy.
    pub(crate) strategy: Arc<dyn RoutingStrategy>,
    /// Shared fault configuration.
    pub(crate) faults: Arc<FaultSet>,
}

impl std::fmt::Debug for AsyncEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncEngine")
            .field("node_count", &self.node_configs.len())
            .field("dimension", &self.cube.dim())
            .field("strategy", &self.strategy.name())
            .field("in_flight", &self.in_flight.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl AsyncEngine {
    /// Builds a new async simulation engine from the given configuration.
    ///
    /// Creates all channels but does **not** spawn async tasks yet.
    /// Tasks are spawned when [`AsyncEngine::run`] is called.
    pub fn build(config: SimulationConfig) -> Self {
        super::builder::build_async(config)
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
            let node_count = self.cube.node_count();
            for packet in &mut packets {
                let intermediate = NodeId::new((intermediate_rng.next_u32() % node_count) as u16);
                packet.phase = Phase::ValiantPhase1 { intermediate };
            }
        }

        // Send each packet to the appropriate node's injection channel.
        for mut packet in packets {
            packet.inject_tick = 0;
            let src_idx = packet.source.raw() as usize;
            if let Err(e) = self.inject_txs[src_idx].try_send(packet) {
                tracing::warn!("failed to inject packet at node {src_idx}: {e}");
            }
            let _ = self.in_flight.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Runs the simulation for `max_ticks` synchronized ticks.
    ///
    /// Spawns all node tasks, participates in the tick barrier as the
    /// orchestrator, then collects and returns aggregated statistics.
    pub async fn run(
        self,
        traffic_name: &str,
        max_ticks: u64,
    ) -> SimulationStats {
        let dim = self.cube.dim();
        let strategy_name = self.strategy.name().to_owned();
        let total_packets = self.in_flight.load(Ordering::Relaxed);
        let node_count = self.node_configs.len();

        // Barrier: all nodes + the orchestrator.
        let barrier = Arc::new(tokio::sync::Barrier::new(node_count + 1));

        // Spawn node tasks now, passing the agreed-upon max_ticks.
        let handles: Vec<_> = self
            .node_configs
            .into_iter()
            .map(|nc| {
                let node = crate::node::Node::new(
                    nc.id,
                    Arc::clone(&self.cube),
                    nc.rx,
                    nc.tx,
                    nc.inject_rx,
                    Arc::clone(&self.strategy),
                    Arc::clone(&self.faults),
                    Arc::clone(&barrier),
                    max_ticks,
                    nc.rng_seed,
                );
                tokio::spawn(node.run())
            })
            .collect();

        // Orchestrator: participate in exactly max_ticks barriers.
        for _ in 0..max_ticks {
            let _ = barrier.wait().await;
        }

        // All nodes have also completed their max_ticks loops.
        // Collect results.
        let mut per_node = Vec::with_capacity(handles.len());
        for handle in handles {
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
