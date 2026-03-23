use rand::RngCore;

use crate::packet::Packet;
use crate::topology::{FaultSet, Hypercube, NodeId};

use super::RoutingStrategy;

/// Bit-fixing (greedy) routing strategy.
///
/// Routes packets by correcting differing bits from the most significant to
/// the least significant. On a fault-free n-cube this delivers every packet
/// in exactly `hamming_distance(source, destination)` hops.
///
/// Worst-case congestion is Theta(sqrt(N)) on the bit-reversal permutation.
#[derive(Debug, Clone, Copy)]
pub struct BitFixing;

impl BitFixing {
    /// Computes the next hop via bit-fixing from `current` toward `target`.
    ///
    /// Corrects the highest-numbered differing bit first (MSB-first). If that
    /// neighbor is faulty, falls back to the next differing bit.
    ///
    /// Returns `None` if `current == target` or all differing-bit neighbors
    /// are faulty.
    pub fn bit_fix_step(
        current: NodeId,
        target: NodeId,
        cube: &Hypercube,
        faults: &FaultSet,
    ) -> Option<NodeId> {
        let diff = current.differing_bits(target);
        if diff == 0 {
            return None;
        }

        // Try dimensions from highest to lowest (MSB-first bit-fixing).
        let mut remaining = diff;
        while remaining != 0 {
            // Highest set bit in remaining.
            let dim = 15 - remaining.leading_zeros() as u8;
            let next = current.flip(dim);
            if dim < cube.dim() && !faults.is_node_failed(next) && !faults.is_link_failed(current, next) {
                return Some(next);
            }
            remaining &= !(1 << dim);
        }

        None
    }
}

impl RoutingStrategy for BitFixing {
    fn next_hop(
        &self,
        packet: &mut Packet,
        current: NodeId,
        cube: &Hypercube,
        faults: &FaultSet,
        _rng: &mut dyn RngCore,
    ) -> Option<NodeId> {
        Self::bit_fix_step(current, packet.destination, cube, faults)
    }

    fn name(&self) -> &'static str {
        "bit-fixing"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::Phase;

    fn make_packet(src: u16, dst: u16) -> Packet {
        Packet {
            id: 0,
            source: NodeId::new(src),
            destination: NodeId::new(dst),
            phase: Phase::Direct,
            hops: 0,
            inject_tick: 0,
            payload: (),
        }
    }

    #[test]
    fn already_at_destination() {
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        assert!(BitFixing::bit_fix_step(NodeId::new(5), NodeId::new(5), &cube, &faults).is_none());
    }

    #[test]
    fn single_bit_difference() {
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        // 0b0000 -> 0b1000: should flip bit 3.
        let next = BitFixing::bit_fix_step(NodeId::new(0b0000), NodeId::new(0b1000), &cube, &faults);
        assert_eq!(next, Some(NodeId::new(0b1000)));
    }

    #[test]
    fn msb_first_ordering() {
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        // 0b0000 -> 0b1111: should fix bit 3 first (MSB).
        let next = BitFixing::bit_fix_step(NodeId::new(0b0000), NodeId::new(0b1111), &cube, &faults);
        assert_eq!(next, Some(NodeId::new(0b1000)));
    }

    #[test]
    fn delivers_in_hamming_distance_hops() {
        let cube = Hypercube::new(6);
        let faults = FaultSet::new();
        let strategy = BitFixing;
        let mut rng = rand::rng();

        // Test all pairs in a 6-cube.
        for src in 0..64u16 {
            for dst in 0..64u16 {
                let expected_hops = NodeId::new(src).hamming_distance(NodeId::new(dst));
                let mut packet = make_packet(src, dst);
                let mut current = NodeId::new(src);
                let mut actual_hops = 0u32;

                while let Some(next) = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng) {
                    current = next;
                    actual_hops += 1;
                    assert!(actual_hops <= expected_hops, "exceeded expected hops");
                }

                assert_eq!(current, NodeId::new(dst));
                assert_eq!(actual_hops, expected_hops);
            }
        }
    }

    #[test]
    fn no_loops() {
        let cube = Hypercube::new(6);
        let faults = FaultSet::new();
        let strategy = BitFixing;
        let mut rng = rand::rng();

        let mut packet = make_packet(0, 63);
        let mut current = NodeId::new(0);
        let mut visited = std::collections::HashSet::new();
        let _ = visited.insert(current);

        while let Some(next) = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng) {
            assert!(visited.insert(next), "loop detected at {next}");
            current = next;
        }
    }

    #[test]
    fn fault_avoidance() {
        let cube = Hypercube::new(4);
        let mut faults = FaultSet::new();

        // 0b0000 -> 0b1100: diff bits are 2 and 3.
        // Normally would fix bit 3 first -> 0b1000.
        // If we fault node 0b1000, should fix bit 2 instead -> 0b0100.
        faults.fail_node(NodeId::new(0b1000));
        let next = BitFixing::bit_fix_step(NodeId::new(0b0000), NodeId::new(0b1100), &cube, &faults);
        assert_eq!(next, Some(NodeId::new(0b0100)));
    }
}
