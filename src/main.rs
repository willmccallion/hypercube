//! Hypercube routing simulator binary.

use std::io::Write;
use std::sync::Arc;

use clap::Parser;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use hypercube::cli::{Cli, Strategy, Traffic};
use hypercube::engine::{Engine, SimulationConfig};
use hypercube::routing::{BitFixing, FaultTolerant, RoutingStrategy, Valiant};
use hypercube::stats::{SimulationStats, write_csv};
use hypercube::topology::FaultSet;
use hypercube::traffic::{
    BitReversal, Butterfly, Complement, Identity, RandomPermutation, Shuffle, TrafficPattern,
    Transpose,
};

fn make_strategy(s: Strategy) -> Arc<dyn RoutingStrategy> {
    match s {
        Strategy::Valiant => Arc::new(Valiant),
        Strategy::FaultTolerant => Arc::new(FaultTolerant),
        Strategy::Bitfix | Strategy::Both => Arc::new(BitFixing),
    }
}

fn make_traffic(t: Traffic) -> Box<dyn TrafficPattern> {
    match t {
        Traffic::BitReversal => Box::new(BitReversal),
        Traffic::Complement => Box::new(Complement),
        Traffic::Identity => Box::new(Identity),
        Traffic::Transpose => Box::new(Transpose),
        Traffic::Random => Box::new(RandomPermutation),
        Traffic::Butterfly => Box::new(Butterfly),
        Traffic::Shuffle => Box::new(Shuffle),
    }
}

fn parse_range(s: &str) -> (u8, u8) {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let lo = parts[0].parse().unwrap_or(3);
        let hi = parts[1].parse().unwrap_or(12);
        (lo, hi)
    } else {
        (3, 12)
    }
}

/// Parses a traffic range string into a list of traffic patterns.
///
/// Accepts `"all"` for every variant, or a comma-separated list of clap value
/// names (e.g., `"bit-reversal,complement,shuffle"`).
fn parse_traffic_range(s: &str) -> Vec<Traffic> {
    use clap::ValueEnum;
    if s.eq_ignore_ascii_case("all") {
        return Traffic::value_variants().to_vec();
    }
    s.split(',')
        .filter_map(|name| Traffic::from_str(name.trim(), true).ok())
        .collect()
}

/// Parses a capacity range string in `start:end:step` format.
///
/// Returns a list of capacities from `start` to `end` inclusive, stepping by
/// `step`. A single value returns a one-element list.
fn parse_capacity_range(s: &str) -> Vec<usize> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => parts[0].parse().ok().map_or_else(Vec::new, |v| vec![v]),
        2 => {
            let start = parts[0].parse().unwrap_or(256);
            let end = parts[1].parse().unwrap_or(256);
            (start..=end).collect()
        }
        _ => {
            let start: usize = parts[0].parse().unwrap_or(256);
            let end: usize = parts[1].parse().unwrap_or(256);
            let step: usize = parts[2].parse().unwrap_or(1).max(1);
            (start..=end).step_by(step).collect()
        }
    }
}

fn run_single(
    dim: u8,
    strategy: Arc<dyn RoutingStrategy>,
    traffic: &dyn TrafficPattern,
    max_ticks: u64,
    seed: u64,
    channel_capacity: usize,
    num_faults: usize,
) -> SimulationStats {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let cube = hypercube::topology::Hypercube::new(dim);
    let faults = if num_faults > 0 {
        FaultSet::random_node_faults(&cube, num_faults, &mut rng)
    } else {
        FaultSet::new()
    };

    let config = SimulationConfig {
        dimension: dim,
        strategy,
        faults,
        channel_capacity,
        seed,
    };

    let mut engine = Engine::build(config);
    engine.inject(traffic, &mut rng);
    let traffic_name = traffic.name();
    engine.run(traffic_name, max_ticks)
}

/// Runs a parameter sweep across dimensions, capacities, traffic patterns,
/// strategies, and runs.
fn run_sweep(cli: &Cli) -> Vec<SimulationStats> {
    let (lo, hi) = parse_range(&cli.dim_range);

    let traffic_variants: Vec<Traffic> = cli
        .traffic_range
        .as_deref()
        .map_or_else(|| vec![cli.traffic], parse_traffic_range);

    let capacities: Vec<usize> = cli
        .capacity_range
        .as_deref()
        .map_or_else(|| vec![cli.channel_capacity], parse_capacity_range);

    let strategies: Vec<(Arc<dyn RoutingStrategy>, &str)> = match cli.strategy {
        Strategy::Both => vec![
            (Arc::new(BitFixing) as Arc<dyn RoutingStrategy>, "bit-fixing"),
            (Arc::new(Valiant), "valiant"),
        ],
        other => {
            let s = make_strategy(other);
            let name = match other {
                Strategy::Bitfix => "bit-fixing",
                Strategy::Valiant => "valiant",
                Strategy::FaultTolerant => "fault-tolerant",
                Strategy::Both => unreachable!(),
            };
            vec![(s, name)]
        }
    };

    let mut results = Vec::new();

    for dim in lo..=hi {
        let max_ticks = u64::from(dim) * 4 + 20;
        for &capacity in &capacities {
            for &traffic_variant in &traffic_variants {
                let traffic = make_traffic(traffic_variant);
                for (strategy, _name) in &strategies {
                    for run_idx in 0..cli.runs {
                        let seed = cli.seed.wrapping_add(u64::from(run_idx));
                        let stats = run_single(
                            dim,
                            Arc::clone(strategy),
                            traffic.as_ref(),
                            max_ticks,
                            seed,
                            capacity,
                            cli.faults,
                        );

                        eprintln!(
                            "dim={dim:>2} nodes={:>5} cap={capacity:<4} strategy={:<10} traffic={:<12} max_congestion={} delivered={}/{}",
                            1u32 << dim,
                            stats.routing_strategy,
                            stats.traffic_pattern,
                            stats.max_congestion,
                            stats.delivered_packets,
                            stats.total_packets,
                        );

                        results.push(stats);
                    }
                }
            }
        }
    }

    results
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let results = if cli.sweep {
        run_sweep(&cli)
    } else {
        let strategy = make_strategy(cli.strategy);
        let traffic = make_traffic(cli.traffic);
        let stats = run_single(
            cli.dim,
            strategy,
            traffic.as_ref(),
            cli.max_ticks,
            cli.seed,
            cli.channel_capacity,
            cli.faults,
        );

        eprintln!(
            "dim={} nodes={} strategy={} traffic={} max_congestion={} delivered={}/{}",
            stats.dimension,
            1u32 << stats.dimension,
            stats.routing_strategy,
            stats.traffic_pattern,
            stats.max_congestion,
            stats.delivered_packets,
            stats.total_packets,
        );

        vec![stats]
    };

    // Write CSV output.
    let mut writer: Box<dyn Write> = if let Some(path) = &cli.output {
        match std::fs::File::create(path) {
            Ok(f) => Box::new(f),
            Err(e) => {
                eprintln!("error creating {path}: {e}");
                return;
            }
        }
    } else {
        Box::new(std::io::stdout())
    };

    for (i, stats) in results.iter().enumerate() {
        if let Err(e) = write_csv(&mut writer, stats, i == 0) {
            eprintln!("error writing CSV: {e}");
            return;
        }
    }
}
