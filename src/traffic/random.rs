use rand::RngCore;

use crate::packet::Packet;
use crate::topology::Hypercube;

use super::{TrafficPattern, make_packet};

/// Uniform random permutation: each node sends to a uniformly random
/// destination such that the mapping is a valid permutation (bijection).
///
/// Generated via the Fisher-Yates shuffle.
#[derive(Debug, Clone, Copy)]
pub struct RandomPermutation;

impl TrafficPattern for RandomPermutation {
    fn generate(&self, cube: &Hypercube, rng: &mut dyn RngCore) -> Vec<Packet> {
        let n = cube.node_count() as usize;
        // Fisher-Yates shuffle to produce a random permutation.
        let mut perm: Vec<u16> = (0..n as u16).collect();
        for i in (1..n).rev() {
            let j = (next_u64(rng) as usize) % (i + 1);
            perm.swap(i, j);
        }

        cube.nodes()
            .enumerate()
            .map(|(i, src)| {
                let dst = crate::topology::NodeId::new(perm[i]);
                make_packet(i as u64, src, dst)
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "random"
    }
}

/// Helper to get a u64 from the RNG trait object.
fn next_u64(rng: &mut dyn RngCore) -> u64 {
    rng.next_u64()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn random_is_permutation() {
        for n in 1..=10u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = RandomPermutation.generate(&cube, &mut rng);

            let node_count = 1usize << n;
            assert_eq!(packets.len(), node_count);

            let sources: HashSet<u16> = packets.iter().map(|p| p.source.raw()).collect();
            let dests: HashSet<u16> = packets.iter().map(|p| p.destination.raw()).collect();
            assert_eq!(sources.len(), node_count);
            assert_eq!(dests.len(), node_count);
        }
    }

    #[test]
    fn different_seeds_produce_different_permutations() {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let cube = Hypercube::new(8);
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(99);

        let p1 = RandomPermutation.generate(&cube, &mut rng1);
        let p2 = RandomPermutation.generate(&cube, &mut rng2);

        let dests1: Vec<u16> = p1.iter().map(|p| p.destination.raw()).collect();
        let dests2: Vec<u16> = p2.iter().map(|p| p.destination.raw()).collect();
        assert_ne!(dests1, dests2, "different seeds should produce different permutations");
    }

    #[test]
    fn deterministic_with_same_seed() {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let cube = Hypercube::new(8);
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(42);

        let p1 = RandomPermutation.generate(&cube, &mut rng1);
        let p2 = RandomPermutation.generate(&cube, &mut rng2);

        let dests1: Vec<u16> = p1.iter().map(|p| p.destination.raw()).collect();
        let dests2: Vec<u16> = p2.iter().map(|p| p.destination.raw()).collect();
        assert_eq!(dests1, dests2, "same seed should produce same permutation");
    }
}
