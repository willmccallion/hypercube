//! Hypercube topology: implicit graph structure via bit arithmetic.
//!
//! No adjacency list is stored. All structural queries — neighbors,
//! connectivity, validity — are computed from [`NodeId`] bit operations.

mod address;
mod faults;

pub use address::NodeId;
pub use faults::FaultSet;

/// An n-dimensional hypercube topology.
///
/// Contains 2^n nodes where each node is identified by an n-bit [`NodeId`].
/// Two nodes are neighbors if and only if their IDs differ in exactly one bit.
#[derive(Debug, Clone)]
pub struct Hypercube {
    dim: u8,
}

impl Hypercube {
    /// Maximum supported dimension (limited by `NodeId`'s `u16` backing).
    pub const MAX_DIM: u8 = 16;

    /// Creates a new hypercube with the given dimension.
    ///
    /// # Panics
    ///
    /// Panics if `dim` exceeds [`Self::MAX_DIM`].
    pub fn new(dim: u8) -> Self {
        assert!(
            dim <= Self::MAX_DIM,
            "dimension {dim} exceeds maximum {}",
            Self::MAX_DIM
        );
        Self { dim }
    }

    /// Returns the dimension of the hypercube.
    pub const fn dim(&self) -> u8 {
        self.dim
    }

    /// Returns the number of nodes (2^dim).
    pub const fn node_count(&self) -> u32 {
        1 << self.dim
    }

    /// Iterates over all node identifiers in order.
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> {
        let count = self.node_count();
        (0..count).map(|i| NodeId::new(i as u16))
    }

    /// Tests whether two nodes are neighbors (differ in exactly one bit
    /// within the cube's dimension).
    pub const fn are_neighbors(&self, a: NodeId, b: NodeId) -> bool {
        let diff = a.differing_bits(b);
        diff != 0 && diff.is_power_of_two() && (diff.trailing_zeros() as u8) < self.dim
    }

    /// Returns the neighbors of a node as `(dimension, neighbor_id)` pairs.
    pub fn neighbors(&self, node: NodeId) -> impl Iterator<Item = (u8, NodeId)> {
        node.neighbors(self.dim)
    }

    /// Tests whether a node ID is valid for this hypercube.
    pub fn is_valid(&self, node: NodeId) -> bool {
        u32::from(node.raw()) < self.node_count()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};

    use super::*;

    #[test]
    fn node_count_is_power_of_two() {
        for dim in 0..=14u8 {
            let cube = Hypercube::new(dim);
            assert_eq!(cube.node_count(), 1 << dim);
        }
    }

    #[test]
    fn nodes_yields_all_valid_ids() {
        for dim in 0..=8u8 {
            let cube = Hypercube::new(dim);
            let nodes: Vec<_> = cube.nodes().collect();
            assert_eq!(nodes.len(), cube.node_count() as usize);
            for node in &nodes {
                assert!(cube.is_valid(*node));
            }
        }
    }

    #[test]
    fn neighbor_symmetry() {
        let cube = Hypercube::new(5);
        for node in cube.nodes() {
            for (_, neighbor) in cube.neighbors(node) {
                assert!(
                    cube.are_neighbors(node, neighbor),
                    "{node} and {neighbor} should be neighbors"
                );
                assert!(
                    cube.are_neighbors(neighbor, node),
                    "neighbor relation should be symmetric"
                );
            }
        }
    }

    #[test]
    fn each_node_has_exactly_n_neighbors() {
        for dim in 1..=8u8 {
            let cube = Hypercube::new(dim);
            for node in cube.nodes() {
                let count = cube.neighbors(node).count();
                assert_eq!(count, usize::from(dim));
            }
        }
    }

    #[test]
    fn bfs_connectivity() {
        // Every node should be reachable from node 0 in an n-cube.
        for dim in 1..=8u8 {
            let cube = Hypercube::new(dim);
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();

            let start = NodeId::new(0);
            let _ = visited.insert(start);
            queue.push_back(start);

            while let Some(current) = queue.pop_front() {
                for (_, neighbor) in cube.neighbors(current) {
                    if visited.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }

            assert_eq!(
                visited.len(),
                cube.node_count() as usize,
                "not all nodes reachable in {dim}-cube"
            );
        }
    }

    #[test]
    fn bfs_diameter_equals_dimension() {
        // The diameter of an n-cube is n: max shortest-path distance
        // between any two nodes.
        for dim in 1..=8u8 {
            let cube = Hypercube::new(dim);
            let mut max_dist = 0u32;

            // BFS from node 0 — the farthest node should be at distance n.
            let mut dist = vec![u32::MAX; cube.node_count() as usize];
            let mut queue = VecDeque::new();

            dist[0] = 0;
            queue.push_back(NodeId::new(0));

            while let Some(current) = queue.pop_front() {
                let d = dist[current.raw() as usize];
                for (_, neighbor) in cube.neighbors(current) {
                    let idx = neighbor.raw() as usize;
                    if dist[idx] == u32::MAX {
                        dist[idx] = d + 1;
                        if dist[idx] > max_dist {
                            max_dist = dist[idx];
                        }
                        queue.push_back(neighbor);
                    }
                }
            }

            assert_eq!(max_dist, u32::from(dim), "diameter of {dim}-cube");
        }
    }

    #[test]
    fn are_neighbors_rejects_non_neighbors() {
        let cube = Hypercube::new(4);
        // Nodes differing in 2 bits are not neighbors.
        assert!(!cube.are_neighbors(NodeId::new(0b0000), NodeId::new(0b0011)));
        // Same node is not a neighbor of itself.
        assert!(!cube.are_neighbors(NodeId::new(5), NodeId::new(5)));
        // Differ in a bit beyond the cube's dimension.
        let cube3 = Hypercube::new(3);
        assert!(!cube3.are_neighbors(NodeId::new(0b0000), NodeId::new(0b1000)));
    }

    #[test]
    fn is_valid_boundary() {
        let cube = Hypercube::new(4);
        assert!(cube.is_valid(NodeId::new(0)));
        assert!(cube.is_valid(NodeId::new(15)));
        assert!(!cube.is_valid(NodeId::new(16)));
    }
}
