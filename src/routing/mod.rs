//! Routing strategies for hypercube networks.
//!
//! Each strategy implements [`RoutingStrategy`], which makes a purely local
//! routing decision: given a packet header at a node, return the next hop or
//! `None` if the packet has arrived.

mod bitfix;
mod fault_tolerant;
mod valiant;

pub use bitfix::BitFixing;
pub use fault_tolerant::FaultTolerant;
pub use valiant::Valiant;

use crate::packet::PacketHeader;
use crate::topology::{FaultSet, Hypercube, NodeId};

/// A routing strategy that computes the next hop for a packet.
///
/// Strategies are shared across all nodes via `Arc<dyn RoutingStrategy>`.
/// They operate on [`PacketHeader`] rather than the full [`Packet<T>`] so
/// that the trait stays non-generic and object-safe regardless of payload type.
///
/// [`Packet<T>`]: crate::packet::Packet
pub trait RoutingStrategy: Send + Sync + 'static {
    /// Returns the next node to forward the packet to, or `None` if the
    /// packet has reached its destination.
    ///
    /// The `current` parameter is the node currently holding the packet.
    /// The strategy may mutate the header (e.g., to transition Valiant phases).
    fn next_hop(
        &self,
        header: &mut PacketHeader,
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
