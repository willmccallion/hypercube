use std::collections::HashSet;

use super::{Hypercube, NodeId};

/// Tracks failed nodes and links in a hypercube.
///
/// Links are stored in canonical form (smaller `NodeId` first) so that
/// `fail_link(a, b)` and `fail_link(b, a)` refer to the same edge.
#[derive(Debug, Clone, Default)]
pub struct FaultSet {
    failed_nodes: HashSet<NodeId>,
    failed_links: HashSet<(NodeId, NodeId)>,
}

/// Returns a link in canonical form (smaller ID first).
const fn canonical_link(a: NodeId, b: NodeId) -> (NodeId, NodeId) {
    if a.raw() <= b.raw() {
        (a, b)
    } else {
        (b, a)
    }
}

impl FaultSet {
    /// Creates an empty fault set with no failures.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks a node as failed.
    pub fn fail_node(&mut self, id: NodeId) {
        let _ = self.failed_nodes.insert(id);
    }

    /// Marks a link between two nodes as failed.
    pub fn fail_link(&mut self, a: NodeId, b: NodeId) {
        let _ = self.failed_links.insert(canonical_link(a, b));
    }

    /// Returns `true` if the given node is failed.
    pub fn is_node_failed(&self, id: NodeId) -> bool {
        self.failed_nodes.contains(&id)
    }

    /// Returns `true` if the link between `a` and `b` is failed.
    pub fn is_link_failed(&self, a: NodeId, b: NodeId) -> bool {
        self.failed_links.contains(&canonical_link(a, b))
    }

    /// Returns the number of failed nodes.
    pub fn failed_node_count(&self) -> usize {
        self.failed_nodes.len()
    }

    /// Returns the number of failed links.
    pub fn failed_link_count(&self) -> usize {
        self.failed_links.len()
    }

    /// Returns `true` if no faults have been registered.
    pub fn is_empty(&self) -> bool {
        self.failed_nodes.is_empty() && self.failed_links.is_empty()
    }

    /// Iterates over healthy neighbors of a node, excluding neighbors
    /// reached via failed links or that are themselves failed.
    pub fn healthy_neighbors<'a>(
        &'a self,
        cube: &'a Hypercube,
        node: NodeId,
    ) -> impl Iterator<Item = (u8, NodeId)> + 'a {
        cube.neighbors(node).filter(move |&(_, neighbor)| {
            !self.is_node_failed(neighbor) && !self.is_link_failed(node, neighbor)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fault_set() {
        let faults = FaultSet::new();
        assert!(faults.is_empty());
        assert_eq!(faults.failed_node_count(), 0);
        assert_eq!(faults.failed_link_count(), 0);
        assert!(!faults.is_node_failed(NodeId::new(0)));
    }

    #[test]
    fn fail_node() {
        let mut faults = FaultSet::new();
        faults.fail_node(NodeId::new(5));

        assert!(faults.is_node_failed(NodeId::new(5)));
        assert!(!faults.is_node_failed(NodeId::new(3)));
        assert_eq!(faults.failed_node_count(), 1);
    }

    #[test]
    fn fail_link_canonical_order() {
        let mut faults = FaultSet::new();
        let a = NodeId::new(3);
        let b = NodeId::new(7);

        // Failing (a, b) and (b, a) should refer to the same link.
        faults.fail_link(a, b);
        assert!(faults.is_link_failed(a, b));
        assert!(faults.is_link_failed(b, a));
        assert_eq!(faults.failed_link_count(), 1);

        // Inserting the reverse should not create a duplicate.
        faults.fail_link(b, a);
        assert_eq!(faults.failed_link_count(), 1);
    }

    #[test]
    fn healthy_neighbors_excludes_faults() {
        let cube = Hypercube::new(3);
        let mut faults = FaultSet::new();

        // Node 0 (0b000) has neighbors: 1 (0b001), 2 (0b010), 4 (0b100).
        let node = NodeId::new(0);

        // All 3 neighbors should be healthy initially.
        assert_eq!(faults.healthy_neighbors(&cube, node).count(), 3);

        // Fail node 1.
        faults.fail_node(NodeId::new(1));
        let healthy: Vec<_> = faults.healthy_neighbors(&cube, node).collect();
        assert_eq!(healthy.len(), 2);
        assert!(healthy.iter().all(|(_, n)| n.raw() != 1));

        // Fail the link 0-2.
        faults.fail_link(NodeId::new(0), NodeId::new(2));
        let healthy: Vec<_> = faults.healthy_neighbors(&cube, node).collect();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].1, NodeId::new(4));
    }

    #[test]
    fn duplicate_faults_are_idempotent() {
        let mut faults = FaultSet::new();
        faults.fail_node(NodeId::new(3));
        faults.fail_node(NodeId::new(3));
        assert_eq!(faults.failed_node_count(), 1);

        faults.fail_link(NodeId::new(0), NodeId::new(1));
        faults.fail_link(NodeId::new(0), NodeId::new(1));
        assert_eq!(faults.failed_link_count(), 1);
    }
}
