//! Traffic patterns for permutation routing experiments.
//!
//! Each pattern assigns a destination to every source node, producing a set
//! of packets that form a permutation routing problem on the hypercube.

mod hotspot;
mod permutations;
mod random;

pub use hotspot::HotSpot;
pub use permutations::{BitReversal, Complement, Identity, Transpose};
pub use random::RandomPermutation;

use rand::RngCore;

use crate::packet::{Packet, Phase};
use crate::topology::{Hypercube, NodeId};

/// A traffic pattern that generates packets for a routing experiment.
pub trait TrafficPattern: Send + Sync {
    /// Generates a mapping from source to destination for every node, then
    /// produces a packet for each pair.
    ///
    /// For Valiant routing, the caller is responsible for setting the
    /// appropriate [`Phase`] after generation.
    fn generate(&self, cube: &Hypercube, rng: &mut dyn RngCore) -> Vec<Packet>;

    /// Returns the human-readable name of this traffic pattern.
    fn name(&self) -> &'static str;
}

/// Builds a packet from a source-destination pair.
///
/// All packets are created in [`Phase::Direct`] by default. The engine
/// assigns Valiant intermediates before injection if needed.
pub(crate) const fn make_packet(id: u64, source: NodeId, destination: NodeId) -> Packet {
    Packet {
        id,
        source,
        destination,
        phase: Phase::Direct,
        hops: 0,
        inject_tick: 0,
    }
}
