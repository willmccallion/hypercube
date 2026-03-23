use rand::RngCore;

use crate::packet::Packet;
use crate::topology::Hypercube;

use super::{TrafficPattern, make_packet};

/// Identity permutation: each node sends to itself.
///
/// Useful as a baseline — all packets arrive in zero hops.
#[derive(Debug, Clone, Copy)]
pub struct Identity;

impl TrafficPattern for Identity {
    fn generate(&self, cube: &Hypercube, _rng: &mut dyn RngCore) -> Vec<Packet> {
        cube.nodes()
            .enumerate()
            .map(|(i, node)| make_packet(i as u64, node, node))
            .collect()
    }

    fn name(&self) -> &'static str {
        "identity"
    }
}

/// Bit-reversal permutation: node `x` sends to `reverse(x)`.
///
/// This is the classic adversarial pattern for bit-fixing routing, producing
/// Theta(sqrt(N)) congestion on the middle links of the hypercube.
#[derive(Debug, Clone, Copy)]
pub struct BitReversal;

impl TrafficPattern for BitReversal {
    fn generate(&self, cube: &Hypercube, _rng: &mut dyn RngCore) -> Vec<Packet> {
        cube.nodes()
            .enumerate()
            .map(|(i, node)| make_packet(i as u64, node, node.bit_reverse(cube.dim())))
            .collect()
    }

    fn name(&self) -> &'static str {
        "bit-reversal"
    }
}

/// Complement permutation: node `x` sends to `NOT(x)` (all bits flipped).
///
/// Every packet must traverse all `n` dimensions, maximizing path length.
#[derive(Debug, Clone, Copy)]
pub struct Complement;

impl TrafficPattern for Complement {
    fn generate(&self, cube: &Hypercube, _rng: &mut dyn RngCore) -> Vec<Packet> {
        let mask = (1u16 << cube.dim()) - 1;
        cube.nodes()
            .enumerate()
            .map(|(i, node)| {
                let dst = crate::topology::NodeId::new(node.raw() ^ mask);
                make_packet(i as u64, node, dst)
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "complement"
    }
}

/// Transpose permutation: swaps the upper and lower halves of the address bits.
///
/// For even `n`, node `x[n-1:n/2] || x[n/2-1:0]` sends to
/// `x[n/2-1:0] || x[n-1:n/2]`. For odd `n`, the middle bit stays in place.
#[derive(Debug, Clone, Copy)]
pub struct Transpose;

impl TrafficPattern for Transpose {
    fn generate(&self, cube: &Hypercube, _rng: &mut dyn RngCore) -> Vec<Packet> {
        let n = cube.dim();
        let half = n / 2;
        let lower_mask = (1u16 << half) - 1;
        let upper_mask = lower_mask << (n - half);

        cube.nodes()
            .enumerate()
            .map(|(i, node)| {
                let raw = node.raw();
                let lower = raw & lower_mask;
                let upper = (raw & upper_mask) >> (n - half);
                let middle = raw & !lower_mask & !upper_mask;
                let dst = crate::topology::NodeId::new((lower << (n - half)) | middle | upper);
                make_packet(i as u64, node, dst)
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "transpose"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_is_permutation(packets: &[Packet], n: u8) {
        let node_count = 1usize << n;
        assert_eq!(packets.len(), node_count);

        let sources: HashSet<u16> = packets.iter().map(|p| p.source.raw()).collect();
        let dests: HashSet<u16> = packets.iter().map(|p| p.destination.raw()).collect();
        assert_eq!(sources.len(), node_count);
        assert_eq!(dests.len(), node_count);
    }

    #[test]
    fn identity_is_permutation() {
        let cube = Hypercube::new(6);
        let mut rng = rand::rng();
        let packets = Identity.generate(&cube, &mut rng);
        assert_is_permutation(&packets, 6);
        for p in &packets {
            assert_eq!(p.source, p.destination);
        }
    }

    #[test]
    fn bit_reversal_is_permutation() {
        for n in 1..=10u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = BitReversal.generate(&cube, &mut rng);
            assert_is_permutation(&packets, n);
        }
    }

    #[test]
    fn bit_reversal_correctness() {
        let cube = Hypercube::new(4);
        let mut rng = rand::rng();
        let packets = BitReversal.generate(&cube, &mut rng);
        // 0b0001 -> 0b1000
        let p = &packets[1];
        assert_eq!(p.destination.raw(), 0b1000);
        // 0b1100 -> 0b0011
        let p = &packets[0b1100];
        assert_eq!(p.destination.raw(), 0b0011);
    }

    #[test]
    fn complement_is_permutation() {
        for n in 1..=10u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = Complement.generate(&cube, &mut rng);
            assert_is_permutation(&packets, n);
        }
    }

    #[test]
    fn complement_max_distance() {
        let cube = Hypercube::new(6);
        let mut rng = rand::rng();
        let packets = Complement.generate(&cube, &mut rng);
        // Every packet should traverse all 6 dimensions.
        for p in &packets {
            assert_eq!(p.source.hamming_distance(p.destination), 6);
        }
    }

    #[test]
    fn transpose_is_permutation() {
        for n in 2..=10u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = Transpose.generate(&cube, &mut rng);
            assert_is_permutation(&packets, n);
        }
    }

    #[test]
    fn transpose_is_involution() {
        // Applying transpose twice should return to the original mapping.
        for n in 2..=8u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = Transpose.generate(&cube, &mut rng);
            for p in &packets {
                // Find the packet whose source is p.destination.
                let second = &packets[p.destination.raw() as usize];
                assert_eq!(second.destination, p.source, "transpose is not involution at n={n}");
            }
        }
    }
}
