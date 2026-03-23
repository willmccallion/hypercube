//! Packet representation for hypercube routing.

use crate::topology::NodeId;

/// A packet being routed through the hypercube.
///
/// The type parameter `T` allows embedding arbitrary payloads (e.g., coherence
/// messages when used as a network-on-chip transport). The default `()` keeps the packet
/// lightweight for standalone routing experiments.
#[derive(Debug, Clone)]
pub struct Packet<T = ()> {
    /// Unique packet identifier.
    pub id: u64,
    /// The node that originally injected this packet.
    pub source: NodeId,
    /// The final destination node.
    pub destination: NodeId,
    /// The routing phase (determines how the next hop is computed).
    pub phase: Phase,
    /// Number of hops taken so far.
    pub hops: u32,
    /// Logical tick at which the packet was injected.
    pub inject_tick: u64,
    /// Application-level payload carried by this packet.
    pub payload: T,
}

/// Routing phase for a packet.
///
/// In bit-fixing, packets use [`Phase::Direct`]. In Valiant's two-phase
/// algorithm, packets start in [`Phase::ValiantPhase1`] toward a random
/// intermediate, then switch to [`Phase::ValiantPhase2`] for the final leg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Route directly to destination via bit-fixing.
    Direct,
    /// Phase 1 of Valiant's algorithm: route to a random intermediate node.
    ValiantPhase1 {
        /// The randomly chosen intermediate destination.
        intermediate: NodeId,
    },
    /// Phase 2 of Valiant's algorithm: route from intermediate to the true
    /// destination.
    ValiantPhase2,
}
