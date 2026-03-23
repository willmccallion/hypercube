use rand::RngCore;

use crate::packet::{Packet, Phase};
use crate::topology::{FaultSet, Hypercube, NodeId};

use super::RoutingStrategy;
use super::bitfix::BitFixing;

/// Valiant's two-phase randomized routing strategy.
///
/// Each packet is first routed to a random intermediate node (Phase 1) via
/// bit-fixing, then from the intermediate to the true destination (Phase 2)
/// via bit-fixing. This load-balances traffic so that the maximum congestion
/// on any link is O(n) with high probability for any permutation routing
/// problem on an n-dimensional hypercube.
///
/// The intermediate node is assigned at packet injection time (stored in the
/// packet's [`Phase::ValiantPhase1`] variant), not inside this strategy.
#[derive(Debug, Clone, Copy)]
pub struct Valiant;

impl RoutingStrategy for Valiant {
    fn next_hop(
        &self,
        packet: &mut Packet,
        current: NodeId,
        cube: &Hypercube,
        faults: &FaultSet,
        _rng: &mut dyn RngCore,
    ) -> Option<NodeId> {
        match packet.phase {
            Phase::ValiantPhase1 { intermediate } => {
                // Route toward the intermediate node.
                if current == intermediate {
                    // Arrived at intermediate — transition to Phase 2.
                    packet.phase = Phase::ValiantPhase2;
                    BitFixing::bit_fix_step(current, packet.destination, cube, faults)
                } else {
                    BitFixing::bit_fix_step(current, intermediate, cube, faults)
                }
            }
            Phase::ValiantPhase2 => {
                // Route toward the true destination.
                BitFixing::bit_fix_step(current, packet.destination, cube, faults)
            }
            Phase::Direct => {
                // Fallback: treat as plain bit-fixing.
                BitFixing::bit_fix_step(current, packet.destination, cube, faults)
            }
        }
    }

    fn name(&self) -> &'static str {
        "valiant"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::NodeId;

    fn make_valiant_packet(src: u16, dst: u16, intermediate: u16) -> Packet {
        Packet {
            id: 0,
            source: NodeId::new(src),
            destination: NodeId::new(dst),
            phase: Phase::ValiantPhase1 {
                intermediate: NodeId::new(intermediate),
            },
            hops: 0,
            inject_tick: 0,
            payload: (),
        }
    }

    #[test]
    fn phase1_routes_to_intermediate() {
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        let strategy = Valiant;
        let mut rng = rand::rng();

        // src=0, dst=15, intermediate=5
        let mut packet = make_valiant_packet(0, 15, 5);
        let mut current = NodeId::new(0);

        // Route until we either arrive at intermediate or run out of hops.
        for _ in 0..20 {
            if let Some(next) = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng) {
                current = next;
            } else {
                break;
            }
            // Check if we transitioned to Phase 2.
            if packet.phase == Phase::ValiantPhase2 {
                break;
            }
        }

        // Should have passed through the intermediate and entered Phase 2.
        assert_eq!(packet.phase, Phase::ValiantPhase2);
    }

    #[test]
    fn full_delivery() {
        let cube = Hypercube::new(6);
        let faults = FaultSet::new();
        let strategy = Valiant;
        let mut rng = rand::rng();

        // Test many (src, dst, intermediate) triples.
        for src in 0..64u16 {
            let dst = src ^ 0b111111; // bit complement
            let intermediate = (src.wrapping_mul(7).wrapping_add(3)) & 0b111111;
            let mut packet = make_valiant_packet(src, dst, intermediate);
            let mut current = NodeId::new(src);
            let mut hops = 0u32;

            while let Some(next) = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng) {
                current = next;
                hops += 1;
                assert!(hops <= 2 * u32::from(cube.dim()), "too many hops");
            }

            assert_eq!(current, NodeId::new(dst), "packet from {src} did not reach {dst}");
        }
    }

    #[test]
    fn total_hops_equals_sum_of_hamming_distances() {
        let cube = Hypercube::new(6);
        let faults = FaultSet::new();
        let strategy = Valiant;
        let mut rng = rand::rng();

        for src in 0..64u16 {
            let dst = 63 - src;
            let intermediate = src ^ 0b101010;

            let expected_hops = NodeId::new(src).hamming_distance(NodeId::new(intermediate))
                + NodeId::new(intermediate).hamming_distance(NodeId::new(dst));

            let mut packet = make_valiant_packet(src, dst, intermediate);
            let mut current = NodeId::new(src);
            let mut actual_hops = 0u32;

            while let Some(next) = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng) {
                current = next;
                actual_hops += 1;
            }

            assert_eq!(actual_hops, expected_hops, "src={src} dst={dst} intermediate={intermediate}");
        }
    }

    #[test]
    fn intermediate_equals_source() {
        // Edge case: intermediate == source → Phase 1 is a no-op, immediately
        // transitions to Phase 2.
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        let strategy = Valiant;
        let mut rng = rand::rng();

        let mut packet = make_valiant_packet(0, 15, 0);
        let current = NodeId::new(0);

        // First call should transition to Phase 2 and return the first hop
        // toward the destination.
        let next = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng);
        assert_eq!(packet.phase, Phase::ValiantPhase2);
        assert!(next.is_some());
    }

    #[test]
    fn intermediate_equals_destination() {
        // Edge case: intermediate == destination → Phase 1 delivers to
        // destination, then Phase 2 is a no-op.
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        let strategy = Valiant;
        let mut rng = rand::rng();

        let mut packet = make_valiant_packet(0, 15, 15);
        let mut current = NodeId::new(0);

        let hamming = NodeId::new(0).hamming_distance(NodeId::new(15));
        let mut hops = 0u32;

        while let Some(next) = strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng) {
            current = next;
            hops += 1;
        }

        assert_eq!(current, NodeId::new(15));
        assert_eq!(hops, hamming);
    }
}
