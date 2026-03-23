use rand::RngCore;

use crate::packet::{Packet, Phase};
use crate::topology::{FaultSet, Hypercube, NodeId};

use super::RoutingStrategy;

/// Fault-tolerant routing strategy for hypercubes.
///
/// Extends bit-fixing with misrouting: when all distance-reducing neighbors
/// are faulty, the packet is forwarded along a randomly chosen healthy
/// non-differing dimension (increasing Hamming distance to the target). This
/// exploits the n-connectivity of the hypercube — an n-cube remains connected
/// as long as fewer than n nodes fail.
///
/// Loop freedom is probabilistic: the random misrouting choice avoids
/// deterministic cycles. The simulation's fixed tick budget provides a hard
/// termination guarantee.
#[derive(Debug, Clone, Copy)]
pub struct FaultTolerant;

impl FaultTolerant {
    /// Attempts to route `current` toward `target`, misrouting if necessary.
    ///
    /// 1. Try all distance-reducing dimensions (MSB-first, same as bit-fixing).
    /// 2. If all are blocked, randomly select a healthy distance-increasing
    ///    neighbor.
    /// 3. Returns `None` only if the current node is completely isolated
    ///    (all neighbors faulty) or `current == target`.
    fn step(
        current: NodeId,
        target: NodeId,
        cube: &Hypercube,
        faults: &FaultSet,
        rng: &mut dyn RngCore,
    ) -> Option<NodeId> {
        // Arrived at target.
        if current == target {
            return None;
        }

        let diff = current.differing_bits(target);

        // Collect healthy distance-reducing neighbors.
        let mut reducing = Vec::new();
        let mut remaining = diff;
        while remaining != 0 {
            let dim = remaining.trailing_zeros() as u8;
            if dim < cube.dim() {
                let next = current.flip(dim);
                if !faults.is_node_failed(next) && !faults.is_link_failed(current, next) {
                    reducing.push(next);
                }
            }
            remaining &= remaining - 1;
        }

        if !reducing.is_empty() {
            // Randomly choose among distance-reducing neighbors to avoid
            // deterministic cycles after misrouting.
            let idx = (rng.next_u32() as usize) % reducing.len();
            return Some(reducing[idx]);
        }

        // All distance-reducing moves blocked. Misroute along a healthy
        // non-differing dimension chosen at random.
        let all_dims_mask = (1u16 << cube.dim()) - 1;
        let non_diff = all_dims_mask & !diff;

        let mut misroute = Vec::new();
        let mut remaining = non_diff;
        while remaining != 0 {
            let dim = remaining.trailing_zeros() as u8;
            let next = current.flip(dim);
            if !faults.is_node_failed(next) && !faults.is_link_failed(current, next) {
                misroute.push(next);
            }
            remaining &= remaining - 1;
        }

        if misroute.is_empty() {
            return None; // node is isolated
        }

        let idx = (rng.next_u32() as usize) % misroute.len();
        Some(misroute[idx])
    }
}

impl RoutingStrategy for FaultTolerant {
    fn next_hop(
        &self,
        packet: &mut Packet,
        current: NodeId,
        cube: &Hypercube,
        faults: &FaultSet,
        rng: &mut dyn RngCore,
    ) -> Option<NodeId> {
        match packet.phase {
            Phase::ValiantPhase1 { intermediate } => {
                if current == intermediate {
                    packet.phase = Phase::ValiantPhase2;
                    Self::step(current, packet.destination, cube, faults, rng)
                } else {
                    Self::step(current, intermediate, cube, faults, rng)
                }
            }
            Phase::ValiantPhase2 | Phase::Direct => {
                Self::step(current, packet.destination, cube, faults, rng)
            }
        }
    }

    fn name(&self) -> &'static str {
        "fault-tolerant"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(src: u16, dst: u16) -> Packet {
        Packet {
            id: 0,
            source: NodeId::new(src),
            destination: NodeId::new(dst),
            phase: Phase::Direct,
            hops: 0,
            inject_tick: 0,
        }
    }

    #[test]
    fn degrades_to_bitfix_without_faults() {
        let cube = Hypercube::new(4);
        let faults = FaultSet::new();
        let strategy = FaultTolerant;
        let mut rng = rand::rng();

        // Without faults, should deliver in exactly hamming-distance hops.
        for src in 0..16u16 {
            for dst in 0..16u16 {
                let expected = NodeId::new(src).hamming_distance(NodeId::new(dst));
                let mut packet = make_packet(src, dst);
                let mut current = NodeId::new(src);
                let mut hops = 0u32;

                while let Some(next) =
                    strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng)
                {
                    current = next;
                    hops += 1;
                    assert!(hops <= expected, "exceeded expected hops without faults");
                }

                assert_eq!(current, NodeId::new(dst));
                assert_eq!(hops, expected);
            }
        }
    }

    #[test]
    fn routes_around_single_fault() {
        let cube = Hypercube::new(4);
        let mut faults = FaultSet::new();
        let strategy = FaultTolerant;
        let mut rng = rand::rng();

        // Route 0b0000 -> 0b1111. Normal path via bit-fixing goes through
        // 0b1000. Block that node.
        faults.fail_node(NodeId::new(0b1000));

        let mut packet = make_packet(0b0000, 0b1111);
        let mut current = NodeId::new(0b0000);
        let mut hops = 0u32;

        while let Some(next) =
            strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng)
        {
            current = next;
            hops += 1;
            assert!(!faults.is_node_failed(current), "routed through a faulty node");
            assert!(hops <= 20, "likely loop detected");
        }

        assert_eq!(current, NodeId::new(0b1111));
    }

    #[test]
    fn routes_around_multiple_faults() {
        let cube = Hypercube::new(5); // 32 nodes, 5-connected
        let mut faults = FaultSet::new();
        let strategy = FaultTolerant;
        let mut rng = rand::rng();

        // Fail 4 nodes (n-1 = 4), leaving the graph connected.
        faults.fail_node(NodeId::new(1));
        faults.fail_node(NodeId::new(2));
        faults.fail_node(NodeId::new(4));
        faults.fail_node(NodeId::new(8));

        // Route between all pairs of surviving nodes.
        for src in 0..32u16 {
            if faults.is_node_failed(NodeId::new(src)) {
                continue;
            }
            for dst in 0..32u16 {
                if faults.is_node_failed(NodeId::new(dst)) {
                    continue;
                }

                let mut packet = make_packet(src, dst);
                let mut current = NodeId::new(src);
                let mut hops = 0u32;

                while let Some(next) =
                    strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng)
                {
                    current = next;
                    hops += 1;
                    assert!(
                        !faults.is_node_failed(current),
                        "routed through faulty node at hop {hops}"
                    );
                    assert!(hops <= 100, "likely loop: {src} -> {dst} after {hops} hops");
                }

                assert_eq!(
                    current,
                    NodeId::new(dst),
                    "failed to deliver {src} -> {dst}"
                );
            }
        }
    }

    #[test]
    fn handles_link_faults() {
        let cube = Hypercube::new(4);
        let mut faults = FaultSet::new();
        let strategy = FaultTolerant;
        let mut rng = rand::rng();

        // Fail the link between 0b0000 and 0b1000 (dimension 3).
        faults.fail_link(NodeId::new(0b0000), NodeId::new(0b1000));

        let mut packet = make_packet(0b0000, 0b1111);
        let mut current = NodeId::new(0b0000);
        let mut hops = 0u32;

        while let Some(next) =
            strategy.next_hop(&mut packet, current, &cube, &faults, &mut rng)
        {
            current = next;
            hops += 1;
            assert!(hops <= 20, "likely loop");
        }

        assert_eq!(current, NodeId::new(0b1111));
    }

    #[test]
    fn isolated_node_returns_none() {
        let cube = Hypercube::new(3);
        let mut faults = FaultSet::new();
        let strategy = FaultTolerant;
        let mut rng = rand::rng();

        // Fail all 3 neighbors of node 0 (nodes 1, 2, 4).
        faults.fail_node(NodeId::new(1));
        faults.fail_node(NodeId::new(2));
        faults.fail_node(NodeId::new(4));

        let mut packet = make_packet(0, 7);
        let result = strategy.next_hop(&mut packet, NodeId::new(0), &cube, &faults, &mut rng);
        assert!(result.is_none(), "isolated node should return None");
    }
}
