//! Metric types for simulation statistics.

use crate::topology::NodeId;

/// Statistics reported by a single node after simulation completes.
#[derive(Debug, Clone)]
pub struct NodeStats {
    /// The node that produced these stats.
    pub id: NodeId,
    /// Total packets forwarded through this node (not counting deliveries).
    pub packets_forwarded: u64,
    /// Packets that reached their final destination at this node.
    pub packets_delivered: u64,
    /// Number of packets sent on each outgoing link, indexed by dimension.
    pub per_link_load: Vec<u64>,
    /// Maximum queue depth observed on each outgoing link, indexed by dimension.
    pub max_queue_depth: Vec<u32>,
    /// Delivery latencies (in ticks) for packets that terminated here.
    pub delivery_latencies: Vec<u64>,
}

/// Aggregated statistics for a complete simulation run.
#[derive(Debug, Clone)]
pub struct SimulationStats {
    /// Hypercube dimension.
    pub dimension: u8,
    /// Name of the routing strategy used.
    pub routing_strategy: String,
    /// Name of the traffic pattern used.
    pub traffic_pattern: String,
    /// Total number of packets injected.
    pub total_packets: u64,
    /// Total number of packets successfully delivered.
    pub delivered_packets: u64,
    /// Maximum load on any single link across all ticks.
    pub max_congestion: u64,
    /// Average per-link load.
    pub avg_congestion: f64,
    /// Maximum delivery latency (in ticks).
    pub max_latency: u64,
    /// Average delivery latency (in ticks).
    pub avg_latency: f64,
    /// Maximum hop count across all delivered packets.
    pub max_hops: u32,
    /// Average hop count across all delivered packets.
    pub avg_hops: f64,
    /// Per-node detailed statistics.
    pub per_node: Vec<NodeStats>,
}

impl SimulationStats {
    /// Aggregates per-node statistics into a simulation summary.
    pub fn aggregate(
        dimension: u8,
        routing_strategy: &str,
        traffic_pattern: &str,
        total_packets: u64,
        per_node: Vec<NodeStats>,
    ) -> Self {
        let delivered_packets: u64 = per_node.iter().map(|n| n.packets_delivered).sum();
        let max_congestion = per_node
            .iter()
            .flat_map(|n| n.per_link_load.iter().copied())
            .max()
            .unwrap_or(0);

        let total_link_load: u64 = per_node
            .iter()
            .flat_map(|n| n.per_link_load.iter().copied())
            .sum();
        let link_count = per_node
            .iter()
            .map(|n| n.per_link_load.len())
            .sum::<usize>();
        let avg_congestion = if link_count > 0 {
            total_link_load as f64 / link_count as f64
        } else {
            0.0
        };

        let all_latencies: Vec<u64> = per_node
            .iter()
            .flat_map(|n| n.delivery_latencies.iter().copied())
            .collect();
        let max_latency = all_latencies.iter().copied().max().unwrap_or(0);
        let avg_latency = if all_latencies.is_empty() {
            0.0
        } else {
            all_latencies.iter().sum::<u64>() as f64 / all_latencies.len() as f64
        };

        // Hop count = forward count along the path. For a delivered packet,
        // the total hops are reflected in the latencies (hops ≤ latency in
        // tick-synchronous mode). We approximate max/avg hops from latencies
        // for now — the engine can provide exact hop counts if needed.
        let max_hops = max_latency as u32;
        let avg_hops = avg_latency;

        Self {
            dimension,
            routing_strategy: routing_strategy.to_owned(),
            traffic_pattern: traffic_pattern.to_owned(),
            total_packets,
            delivered_packets,
            max_congestion,
            avg_congestion,
            max_latency,
            avg_latency,
            max_hops,
            avg_hops,
            per_node,
        }
    }
}
