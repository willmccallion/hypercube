use std::io::Write;

use crate::stats::SimulationStats;

/// Writes simulation results as a CSV row to the given writer.
///
/// The header is written on the first call (when `write_header` is true).
///
/// # Errors
///
/// Returns an error if the underlying write fails.
pub fn write_csv(
    writer: &mut impl Write,
    stats: &SimulationStats,
    write_header: bool,
) -> std::io::Result<()> {
    if write_header {
        writeln!(
            writer,
            "dimension,nodes,strategy,traffic,total_packets,delivered,\
             max_congestion,avg_congestion,max_latency,avg_latency,\
             max_hops,avg_hops,per_dim_load,per_dim_max_queue"
        )?;
    }

    let dim_load: String = stats
        .per_dim_load
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(";");
    let dim_queue: String = stats
        .per_dim_max_queue
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(";");

    writeln!(
        writer,
        "{},{},{},{},{},{},{},{:.2},{},{:.2},{},{:.2},{},{}",
        stats.dimension,
        1u32 << stats.dimension,
        stats.routing_strategy,
        stats.traffic_pattern,
        stats.total_packets,
        stats.delivered_packets,
        stats.max_congestion,
        stats.avg_congestion,
        stats.max_latency,
        stats.avg_latency,
        stats.max_hops,
        stats.avg_hops,
        dim_load,
        dim_queue,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::metrics::NodeStats;
    use crate::topology::NodeId;

    #[test]
    fn csv_round_trip() {
        let node_stats = vec![NodeStats {
            id: NodeId::new(0),
            packets_forwarded: 5,
            packets_delivered: 1,
            per_link_load: vec![3, 2],
            max_queue_depth: vec![1, 1],
            delivery_latencies: vec![4],
            delivery_hops: vec![4],
        }];

        let stats = SimulationStats::aggregate(2, "bit-fixing", "bit-reversal", 4, node_stats);

        let mut buf = Vec::new();
        let _ = write_csv(&mut buf, &stats, true);
        let output = String::from_utf8(buf).ok().unwrap_or_default();

        assert!(output.contains("dimension,"));
        assert!(output.contains("bit-fixing"));
        assert!(output.contains("bit-reversal"));
    }
}
