use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use tokio::sync::{Barrier, mpsc};

use crate::node::Node;
use crate::routing::RoutingStrategy;
use crate::topology::{FaultSet, Hypercube};

use super::Engine;

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

/// Builds the simulation engine: creates channels, spawns node tasks.
pub(super) fn build(config: SimulationConfig) -> Engine {
    let cube = Arc::new(Hypercube::new(config.dimension));
    let dim = config.dimension as usize;
    let node_count = cube.node_count() as usize;
    let faults = Arc::new(config.faults);
    let done = Arc::new(AtomicBool::new(false));
    let in_flight = Arc::new(AtomicU64::new(0));
    // +1 for the orchestrator task.
    let barrier = Arc::new(Barrier::new(node_count + 1));

    // Create per-node injection channels.
    let mut inject_txs = Vec::with_capacity(node_count);
    let mut inject_rxs = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let (tx, rx) = mpsc::channel(config.channel_capacity);
        inject_txs.push(tx);
        inject_rxs.push(Some(rx));
    }

    // Create inter-node channels.
    // For each node and each dimension, we need a channel from that node
    // to its neighbor along that dimension.
    //
    // channel_senders[node_idx][dim] = sender to neighbor along dim
    // channel_receivers[node_idx][dim] = receiver from neighbor along dim
    //
    // The channel from node A to node B along dimension d:
    //   - sender is held by A (to send to B)
    //   - receiver is held by B (to receive from A)
    //
    // Since A's neighbor along dim d is A^(1<<d), and that neighbor's
    // neighbor along dim d is A again, we create one channel pair per
    // (node, dim) and cross-wire them.

    // For each (node, dim), store Option<Sender> and Option<Receiver>.
    let mut senders: Vec<Vec<Option<mpsc::Sender<crate::packet::Packet>>>> =
        (0..node_count).map(|_| (0..dim).map(|_| None).collect()).collect();
    let mut receivers: Vec<Vec<Option<mpsc::Receiver<crate::packet::Packet>>>> =
        (0..node_count).map(|_| (0..dim).map(|_| None).collect()).collect();

    for node_idx in 0..node_count {
        for d in 0..dim {
            // Only create the channel if we haven't already (from the other side).
            if senders[node_idx][d].is_some() {
                continue;
            }

            let neighbor_idx = node_idx ^ (1 << d);

            let (tx_a_to_b, rx_a_to_b) = mpsc::channel(config.channel_capacity);
            let (tx_b_to_a, rx_b_to_a) = mpsc::channel(config.channel_capacity);

            // A sends to B along dim d; B receives from A along dim d.
            senders[node_idx][d] = Some(tx_a_to_b);
            receivers[neighbor_idx][d] = Some(rx_a_to_b);

            // B sends to A along dim d; A receives from B along dim d.
            senders[neighbor_idx][d] = Some(tx_b_to_a);
            receivers[node_idx][d] = Some(rx_b_to_a);
        }
    }

    // Spawn node tasks.
    let mut handles = Vec::with_capacity(node_count);
    for node_idx in 0..node_count {
        let node_tx: Vec<_> = senders[node_idx]
            .iter_mut()
            .map(|s| s.take().unwrap_or_else(|| {
                // This should never happen if wiring is correct.
                let (tx, _rx) = mpsc::channel(1);
                tx
            }))
            .collect();

        let node_rx: Vec<_> = receivers[node_idx]
            .iter_mut()
            .map(|r| r.take().unwrap_or_else(|| {
                // This should never happen if wiring is correct.
                let (_tx, rx) = mpsc::channel(1);
                rx
            }))
            .collect();

        let inject_rx = inject_rxs[node_idx].take().unwrap_or_else(|| {
            let (_tx, rx) = mpsc::channel(1);
            rx
        });

        let node = Node::new(
            crate::topology::NodeId::new(node_idx as u16),
            Arc::clone(&cube),
            node_rx,
            node_tx,
            inject_rx,
            Arc::clone(&config.strategy),
            Arc::clone(&faults),
            Arc::clone(&barrier),
            Arc::clone(&done),
            config.seed.wrapping_add(node_idx as u64),
        );

        let handle = tokio::spawn(node.run());
        handles.push(handle);
    }

    Engine {
        inject_txs,
        handles,
        done,
        in_flight,
        barrier,
        cube,
        strategy: config.strategy,
    }
}
