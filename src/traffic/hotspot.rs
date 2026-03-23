use rand::RngCore;

use crate::packet::Packet;
use crate::topology::{Hypercube, NodeId};

use super::{TrafficPattern, make_packet};

/// Hot-spot traffic pattern: a fraction of nodes send to a single target,
/// the rest send to themselves.
///
/// This models congestion hot-spots in real networks where one popular
/// service node attracts disproportionate traffic.
#[derive(Debug, Clone)]
pub struct HotSpot {
    /// The fraction of nodes (0.0 to 1.0) that target the hot-spot.
    fraction: f64,
    /// The hot-spot destination node.
    target: NodeId,
}

impl HotSpot {
    /// Creates a new hot-spot pattern.
    ///
    /// `fraction` is clamped to [0.0, 1.0]. `target` is the node that
    /// receives the concentrated traffic.
    pub const fn new(fraction: f64, target: NodeId) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            target,
        }
    }
}

impl TrafficPattern for HotSpot {
    fn generate(&self, cube: &Hypercube, rng: &mut dyn RngCore) -> Vec<Packet> {
        let threshold = (self.fraction * f64::from(u32::MAX)) as u32;

        cube.nodes()
            .enumerate()
            .map(|(i, src)| {
                let dst = if (rng.next_u32()) < threshold {
                    self.target
                } else {
                    src
                };
                make_packet(i as u64, src, dst)
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "hot-spot"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn full_hotspot_all_target() {
        let cube = Hypercube::new(4);
        let target = NodeId::new(7);
        let pattern = HotSpot::new(1.0, target);
        let mut rng = ChaCha8Rng::seed_from_u64(0);

        let packets = pattern.generate(&cube, &mut rng);
        for p in &packets {
            assert_eq!(p.destination, target);
        }
    }

    #[test]
    fn zero_hotspot_all_identity() {
        let cube = Hypercube::new(4);
        let pattern = HotSpot::new(0.0, NodeId::new(7));
        let mut rng = ChaCha8Rng::seed_from_u64(0);

        let packets = pattern.generate(&cube, &mut rng);
        for p in &packets {
            assert_eq!(p.source, p.destination);
        }
    }

    #[test]
    fn partial_hotspot_fraction() {
        let cube = Hypercube::new(10); // 1024 nodes
        let target = NodeId::new(0);
        let fraction = 0.5;
        let pattern = HotSpot::new(fraction, target);
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let packets = pattern.generate(&cube, &mut rng);
        let hotspot_count = packets.iter().filter(|p| p.destination == target).count();

        // With 1024 nodes and 50% fraction, expect ~512 targeting the hot-spot.
        // Allow generous tolerance for randomness.
        assert!(
            (400..=624).contains(&hotspot_count),
            "expected ~512 hot-spot packets, got {hotspot_count}"
        );
    }
}
