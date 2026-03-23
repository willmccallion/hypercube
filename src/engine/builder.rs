use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tokio::sync::mpsc;

use crate::routing::RoutingStrategy;
use crate::topology::{FaultSet, Hypercube};

use super::{Engine, NodeConfig};

/// Configuration for building a simulation engine.
pub struct SimulationConfig {
    /// Hypercube dimension.
    pub dimension: u8,
    /// The routing strategy to use.
    pub strategy: Arc<dyn RoutingStrategy>,
    /// Fault configuration (empty for fault-free simulation).
    pub faults: FaultSet,
    /// Channel buffer capacity per link.
    pub channel_capacity: usize,
    /// Base RNG seed for reproducibility.
    pub seed: u64,
}

impl std::fmt::Debug for SimulationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulationConfig")
            .field("dimension", &self.dimension)
            .field("strategy", &self.strategy.name())
            .field("faults", &self.faults)
            .field("channel_capacity", &self.channel_capacity)
            .field("seed", &self.seed)
            .finish()
    }
}

/// Builds the simulation engine: creates channels and pre-assembles node
/// configs. Tasks are **not** spawned here — that happens in [`Engine::run`].
pub(super) fn build(config: SimulationConfig) -> Engine {
    let cube = Arc::new(Hypercube::new(config.dimension));
    let dim = config.dimension as usize;
    let node_count = cube.node_count() as usize;
    let faults = Arc::new(config.faults);
    let in_flight = Arc::new(AtomicU64::new(0));

    // Create per-node injection channels.
    let mut inject_txs = Vec::with_capacity(node_count);
    let mut inject_rxs: Vec<Option<mpsc::Receiver<crate::packet::Packet>>> =
        Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let (tx, rx) = mpsc::channel(config.channel_capacity);
        inject_txs.push(tx);
        inject_rxs.push(Some(rx));
    }

    // Create inter-node channels.
    //
    // For each edge (node, neighbor) along dimension d, we create a channel
    // pair. The sender is held by the source node, the receiver by the
    // destination node. Each undirected edge produces two directed channels.
    let mut senders: Vec<Vec<Option<mpsc::Sender<crate::packet::Packet>>>> =
        (0..node_count)
            .map(|_| (0..dim).map(|_| None).collect())
            .collect();
    let mut receivers: Vec<Vec<Option<mpsc::Receiver<crate::packet::Packet>>>> =
        (0..node_count)
            .map(|_| (0..dim).map(|_| None).collect())
            .collect();

    for node_idx in 0..node_count {
        for d in 0..dim {
            if senders[node_idx][d].is_some() {
                continue;
            }

            let neighbor_idx = node_idx ^ (1 << d);

            let (tx_a_to_b, rx_a_to_b) = mpsc::channel(config.channel_capacity);
            let (tx_b_to_a, rx_b_to_a) = mpsc::channel(config.channel_capacity);

            senders[node_idx][d] = Some(tx_a_to_b);
            receivers[neighbor_idx][d] = Some(rx_a_to_b);

            senders[neighbor_idx][d] = Some(tx_b_to_a);
            receivers[node_idx][d] = Some(rx_b_to_a);
        }
    }

    // Assemble per-node configs (channels + metadata, no spawning yet).
    let mut node_configs = Vec::with_capacity(node_count);
    for node_idx in 0..node_count {
        let node_tx: Vec<_> = senders[node_idx]
            .iter_mut()
            .map(|s| {
                s.take().unwrap_or_else(|| {
                    let (tx, _rx) = mpsc::channel(1);
                    tx
                })
            })
            .collect();

        let node_rx: Vec<_> = receivers[node_idx]
            .iter_mut()
            .map(|r| {
                r.take().unwrap_or_else(|| {
                    let (_tx, rx) = mpsc::channel(1);
                    rx
                })
            })
            .collect();

        let inject_rx = inject_rxs[node_idx].take().unwrap_or_else(|| {
            let (_tx, rx) = mpsc::channel(1);
            rx
        });

        node_configs.push(NodeConfig {
            id: crate::topology::NodeId::new(node_idx as u16),
            rx: node_rx,
            tx: node_tx,
            inject_rx,
            rng_seed: config.seed.wrapping_add(node_idx as u64),
        });
    }

    Engine {
        node_configs,
        inject_txs,
        in_flight,
        cube,
        strategy: config.strategy,
        faults,
    }
}
