//! Routing strategies for hypercube networks.
//!
//! Each strategy implements [`RoutingStrategy`], which makes a purely local
//! routing decision: given a packet at a node, return the next hop or `None`
//! if the packet has arrived.

mod bitfix;
mod valiant;

pub use bitfix::BitFixing;
pub use valiant::Valiant;

use crate::packet::Packet;
use crate::topology::{FaultSet, Hypercube, NodeId};

/// A routing strategy that computes the next hop for a packet.
///
/// Strategies are shared across all node tasks via `Arc<dyn RoutingStrategy>`.
/// Randomness is provided by the caller so the strategy itself is `Sync`.
pub trait RoutingStrategy: Send + Sync + 'static {
    /// Returns the next node to forward the packet to, or `None` if the
    /// packet has reached its destination.
    ///
    /// The `current` parameter is the node currently holding the packet.
    /// The strategy may mutate the packet (e.g., to transition Valiant phases).
    fn next_hop(
        &self,
        packet: &mut Packet,
        current: NodeId,
        cube: &Hypercube,
        faults: &FaultSet,
        rng: &mut dyn RngCore,
    ) -> Option<NodeId>;

    /// Returns the human-readable name of this strategy.
    fn name(&self) -> &'static str;
}

/// Re-export for use in strategy implementations.
use rand::RngCore;
