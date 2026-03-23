use std::fmt;

/// A node identifier in an n-dimensional hypercube.
///
/// Each node is addressed by an n-bit integer where neighbors differ by exactly
/// one bit. The underlying `u16` supports hypercubes up to dimension 16 (65,536
/// nodes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u16);

impl NodeId {
    /// Creates a new node identifier from a raw integer.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the raw integer value.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Tests the bit at the given dimension.
    pub const fn bit(self, dim: u8) -> bool {
        (self.0 >> dim) & 1 != 0
    }

    /// Returns the neighbor along the given dimension by flipping bit `dim`.
    pub const fn flip(self, dim: u8) -> Self {
        Self(self.0 ^ (1 << dim))
    }

    /// Computes the Hamming distance to another node.
    pub const fn hamming_distance(self, other: Self) -> u32 {
        (self.0 ^ other.0).count_ones()
    }

    /// Returns a bitmask of dimensions where `self` and `other` differ.
    pub const fn differing_bits(self, other: Self) -> u16 {
        self.0 ^ other.0
    }

    /// Reverses the lowest `n` bits, producing the bit-reversal permutation
    /// target.
    pub const fn bit_reverse(self, n: u8) -> Self {
        let mut result: u16 = 0;
        let mut i: u8 = 0;
        while i < n {
            if (self.0 >> i) & 1 != 0 {
                result |= 1 << (n - 1 - i);
            }
            i += 1;
        }
        Self(result)
    }

    /// Iterates over all neighbors in an `n`-dimensional hypercube.
    ///
    /// Yields `(dimension, neighbor_id)` pairs for each of the `n` dimensions.
    pub fn neighbors(self, n: u8) -> impl Iterator<Item = (u8, Self)> {
        (0..n).map(move |dim| (dim, self.flip(dim)))
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flip_is_involution() {
        for dim in 0..16u8 {
            let id = NodeId::new(0b1010_1010);
            assert_eq!(id.flip(dim).flip(dim), id);
        }
    }

    #[test]
    fn flip_changes_exactly_one_bit() {
        let id = NodeId::new(0);
        for dim in 0..8u8 {
            let flipped = id.flip(dim);
            assert_eq!(id.hamming_distance(flipped), 1);
            assert_eq!(id.differing_bits(flipped), 1 << dim);
        }
    }

    #[test]
    fn neighbors_count_and_uniqueness() {
        for n in 1..=10u8 {
            let id = NodeId::new(0);
            let neighbors: Vec<_> = id.neighbors(n).collect();
            assert_eq!(neighbors.len(), usize::from(n));

            // All neighbors are distinct.
            let mut seen = std::collections::HashSet::new();
            for (_, neighbor) in &neighbors {
                assert!(seen.insert(*neighbor));
            }

            // Each neighbor is at Hamming distance 1.
            for (dim, neighbor) in &neighbors {
                assert_eq!(id.hamming_distance(*neighbor), 1);
                assert_eq!(id.differing_bits(*neighbor), 1 << dim);
            }
        }
    }

    #[test]
    fn hamming_distance_is_metric() {
        let a = NodeId::new(0b1100);
        let b = NodeId::new(0b1010);
        let c = NodeId::new(0b0110);

        // Identity of indiscernibles.
        assert_eq!(a.hamming_distance(a), 0);

        // Symmetry.
        assert_eq!(a.hamming_distance(b), b.hamming_distance(a));

        // Triangle inequality.
        assert!(a.hamming_distance(c) <= a.hamming_distance(b) + b.hamming_distance(c));
    }

    #[test]
    fn differing_bits_matches_hamming() {
        let a = NodeId::new(0b1100_1010);
        let b = NodeId::new(0b0110_0101);
        assert_eq!(
            a.differing_bits(b).count_ones(),
            a.hamming_distance(b)
        );
    }

    #[test]
    fn bit_reverse_examples() {
        // 4-bit reversal: 0b0001 -> 0b1000
        assert_eq!(NodeId::new(0b0001).bit_reverse(4), NodeId::new(0b1000));
        // 4-bit reversal: 0b1100 -> 0b0011
        assert_eq!(NodeId::new(0b1100).bit_reverse(4), NodeId::new(0b0011));
        // 4-bit reversal: 0b1010 -> 0b0101
        assert_eq!(NodeId::new(0b1010).bit_reverse(4), NodeId::new(0b0101));
        // Identity for palindromic bit patterns.
        assert_eq!(NodeId::new(0b0110).bit_reverse(4), NodeId::new(0b0110));
    }

    #[test]
    fn bit_reverse_is_involution() {
        for n in 1..=10u8 {
            let node_count = 1u16 << n;
            for raw in 0..node_count {
                let id = NodeId::new(raw);
                assert_eq!(id.bit_reverse(n).bit_reverse(n), id);
            }
        }
    }

    #[test]
    fn bit_reverse_is_permutation() {
        for n in 1..=10u8 {
            let node_count = 1u16 << n;
            let mut targets: Vec<u16> = (0..node_count)
                .map(|raw| NodeId::new(raw).bit_reverse(n).raw())
                .collect();
            targets.sort_unstable();
            let expected: Vec<u16> = (0..node_count).collect();
            assert_eq!(targets, expected);
        }
    }

    #[test]
    fn bit_accessor() {
        let id = NodeId::new(0b1010);
        assert!(!id.bit(0));
        assert!(id.bit(1));
        assert!(!id.bit(2));
        assert!(id.bit(3));
    }
}
