//! Command-line interface for the hypercube routing simulator.

use std::num::NonZeroUsize;

use clap::{Parser, ValueEnum};

use crate::engine::DEFAULT_CHANNEL_CAPACITY;

/// Distributed hypercube routing simulator.
///
/// Simulates packet routing on n-dimensional hypercubes and measures
/// congestion, latency, and path length against theoretical bounds.
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    /// Hypercube dimension (number of nodes = 2^dim).
    #[arg(short, long, default_value_t = 6)]
    pub dim: u8,

    /// Routing strategy.
    #[arg(short, long, value_enum, default_value_t = Strategy::Valiant)]
    pub strategy: Strategy,

    /// Traffic pattern.
    #[arg(short, long, value_enum, default_value_t = Traffic::BitReversal)]
    pub traffic: Traffic,

    /// Tick limit; a run stops earlier once every packet is delivered.
    #[arg(short, long, default_value_t = 100_000)]
    pub max_ticks: u64,

    /// RNG seed for reproducibility.
    #[arg(long, default_value_t = 42)]
    pub seed: u64,

    /// Output CSV file path (omit to print to stdout).
    #[arg(short, long)]
    pub output: Option<String>,

    /// Run a sweep across dimensions and strategies.
    #[arg(long)]
    pub sweep: bool,

    /// Dimension range for sweep mode (e.g., "3:12").
    #[arg(long, default_value = "3:12")]
    pub dim_range: String,

    /// Number of runs per configuration in sweep mode.
    #[arg(long, default_value_t = 1)]
    pub runs: u32,

    /// Packets that can queue on each outgoing link.
    #[arg(long, default_value_t = DEFAULT_CHANNEL_CAPACITY)]
    pub channel_capacity: NonZeroUsize,

    /// Number of random node faults to inject.
    #[arg(long, default_value_t = 0)]
    pub faults: usize,

    /// Traffic patterns for sweep mode ("all" or comma-separated, e.g.,
    /// "bit-reversal,complement,shuffle").
    #[arg(long)]
    pub traffic_range: Option<String>,

    /// Channel capacity range for sweep mode (start:end:step, e.g., "64:512:64").
    #[arg(long, value_parser = parse_capacity_range)]
    pub capacity_range: Option<Vec<NonZeroUsize>>,
}

/// Parses `n`, `start:end` or `start:end:step` into the capacities to sweep.
fn parse_capacity_range(s: &str) -> Result<Vec<NonZeroUsize>, String> {
    let parse = |part: &str| {
        part.parse::<NonZeroUsize>()
            .map_err(|e| format!("invalid capacity {part:?}: {e}"))
    };
    let parts: Vec<&str> = s.split(':').collect();
    let (start, end, step) = match parts.as_slice() {
        [single] => return Ok(vec![parse(single)?]),
        [start, end] => (parse(start)?, parse(end)?, NonZeroUsize::MIN),
        [start, end, step] => (parse(start)?, parse(end)?, parse(step)?),
        _ => return Err(format!("expected n, a:b or a:b:step, got {s:?}")),
    };
    if start > end {
        return Err(format!("range start {start} is above its end {end}"));
    }
    Ok((start.get()..=end.get())
        .step_by(step.get())
        .filter_map(NonZeroUsize::new)
        .collect())
}

/// Routing strategy selection.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Strategy {
    /// Bit-fixing (greedy, MSB-first).
    Bitfix,
    /// Valiant's two-phase randomized routing.
    Valiant,
    /// Fault-tolerant routing with misrouting.
    FaultTolerant,
    /// Run both bit-fixing and Valiant (for sweep mode).
    Both,
}

/// Traffic pattern selection.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Traffic {
    /// Each node sends to its bit-reversal.
    BitReversal,
    /// Each node sends to its bitwise complement.
    Complement,
    /// Each node sends to itself (baseline).
    Identity,
    /// Swap upper and lower address halves.
    Transpose,
    /// Uniform random permutation.
    Random,
    /// Swap bit 0 and bit n-1 (butterfly cross-wiring).
    Butterfly,
    /// Circular left-shift of the address bits (perfect shuffle).
    Shuffle,
}
