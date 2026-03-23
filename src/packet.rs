//! Packet representation for hypercube routing.

use crate::topology::NodeId;

/// Routing-relevant fields of a packet.
///
/// This is the subset of [`Packet`] that routing strategies inspect and
/// modify. Extracting it into its own type allows [`RoutingStrategy`] to
/// remain non-generic — it operates on `&mut PacketHeader` regardless of
/// what payload type `T` the surrounding [`Packet<T>`] carries.
///
/// [`RoutingStrategy`]: crate::routing::RoutingStrategy
#[derive(Debug, Clone)]
pub struct PacketHeader {
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
}

/// A packet being routed through the hypercube.
///
/// The type parameter `T` allows embedding arbitrary payloads (e.g., coherence
/// messages when used as a network-on-chip transport). The default `()` keeps
/// the packet lightweight for standalone routing experiments.
///
/// Routing-relevant fields live in [`PacketHeader`] and are accessible via
/// [`Deref`] — e.g., `packet.destination` works directly.
#[derive(Debug, Clone)]
pub struct Packet<T = ()> {
    /// Routing header (source, destination, phase, hops, etc.).
    pub header: PacketHeader,
    /// Application-level payload carried by this packet.
    pub payload: T,
}

impl<T> std::ops::Deref for Packet<T> {
    type Target = PacketHeader;
    fn deref(&self) -> &PacketHeader {
        &self.header
    }
}

impl<T> std::ops::DerefMut for Packet<T> {
    fn deref_mut(&mut self) -> &mut PacketHeader {
        &mut self.header
    }
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
