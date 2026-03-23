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

/// Butterfly permutation: swaps bit 0 and bit `n − 1` of the address.
///
/// This models the cross-wiring in a butterfly (FFT) network's first and last
/// stages. It is an involution (applying it twice yields the identity). For
/// `n = 1` the permutation is the identity.
#[derive(Debug, Clone, Copy)]
pub struct Butterfly;

impl TrafficPattern for Butterfly {
    fn generate(&self, cube: &Hypercube, _rng: &mut dyn RngCore) -> Vec<Packet> {
        let n = cube.dim();
        cube.nodes()
            .enumerate()
            .map(|(i, node)| {
                let raw = node.raw();
                let dst = if n <= 1 {
                    raw
                } else {
                    let b0 = raw & 1;
                    let bn = (raw >> (n - 1)) & 1;
                    (raw & !(1 | (1 << (n - 1)))) | (b0 << (n - 1)) | bn
                };
                make_packet(i as u64, node, crate::topology::NodeId::new(dst))
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "butterfly"
    }
}

/// Perfect-shuffle permutation: circular left-shift of the `n`-bit address.
///
/// `shuffle(x) = (x << 1 | x >> (n − 1)) & mask`. This models the
/// interconnection pattern of a shuffle-exchange network. Applying it `n`
/// times returns to the identity.
#[derive(Debug, Clone, Copy)]
pub struct Shuffle;

impl TrafficPattern for Shuffle {
    fn generate(&self, cube: &Hypercube, _rng: &mut dyn RngCore) -> Vec<Packet> {
        let n = cube.dim();
        let mask = (1u16 << n) - 1;
        cube.nodes()
            .enumerate()
            .map(|(i, node)| {
                let raw = node.raw();
                let dst = if n <= 1 {
                    raw
                } else {
                    ((raw << 1) | (raw >> (n - 1))) & mask
                };
                make_packet(i as u64, node, crate::topology::NodeId::new(dst))
            })
            .collect()
    }

    fn name(&self) -> &'static str {
        "shuffle"
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

    #[test]
    fn butterfly_is_permutation() {
        for n in 1..=10u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = Butterfly.generate(&cube, &mut rng);
            assert_is_permutation(&packets, n);
        }
    }

    #[test]
    fn butterfly_is_involution() {
        for n in 1..=8u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = Butterfly.generate(&cube, &mut rng);
            for p in &packets {
                let second = &packets[p.destination.raw() as usize];
                assert_eq!(second.destination, p.source, "butterfly is not involution at n={n}");
            }
        }
    }

    #[test]
    fn butterfly_correctness() {
        // n=4: swap bit 0 and bit 3
        // 0b0001 (1) -> swap bit0=1 and bit3=0 -> 0b1000 (8)
        // 0b1010 (10) -> swap bit0=0 and bit3=1 -> 0b0011 (3)
        let cube = Hypercube::new(4);
        let mut rng = rand::rng();
        let packets = Butterfly.generate(&cube, &mut rng);
        assert_eq!(packets[0b0001].destination.raw(), 0b1000);
        assert_eq!(packets[0b1010].destination.raw(), 0b0011);
        // Node 0 maps to itself (both endpoint bits are 0).
        assert_eq!(packets[0].destination.raw(), 0);
    }

    #[test]
    fn shuffle_is_permutation() {
        for n in 1..=10u8 {
            let cube = Hypercube::new(n);
            let mut rng = rand::rng();
            let packets = Shuffle.generate(&cube, &mut rng);
            assert_is_permutation(&packets, n);
        }
    }

    #[test]
    fn shuffle_period_is_n() {
        // Applying the shuffle n times should return to identity.
        for n in 2..=8u8 {
            let node_count = 1u16 << n;
            let mask = node_count - 1;
            for x in 0..node_count {
                let mut val = x;
                for _ in 0..n {
                    val = ((val << 1) | (val >> (n - 1))) & mask;
                }
                assert_eq!(val, x, "shuffle^{n} != identity for x={x}");
            }
        }
    }

    #[test]
    fn shuffle_correctness() {
        // n=4: left-rotate 4-bit address by 1
        // 0b0001 (1) -> 0b0010 (2)
        // 0b1000 (8) -> 0b0001 (1)
        // 0b1010 (10) -> 0b0101 (5)
        let cube = Hypercube::new(4);
        let mut rng = rand::rng();
        let packets = Shuffle.generate(&cube, &mut rng);
        assert_eq!(packets[0b0001].destination.raw(), 0b0010);
        assert_eq!(packets[0b1000].destination.raw(), 0b0001);
        assert_eq!(packets[0b1010].destination.raw(), 0b0101);
    }
}
