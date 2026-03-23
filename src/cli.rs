//! Command-line interface for the hypercube routing simulator.

use clap::{Parser, ValueEnum};

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

    /// Maximum simulation ticks.
    #[arg(short, long, default_value_t = 100)]
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

    /// Channel buffer capacity per inter-node link.
    #[arg(long, default_value_t = 256)]
    pub channel_capacity: usize,
}

/// Routing strategy selection.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Strategy {
    /// Bit-fixing (greedy, MSB-first).
    Bitfix,
    /// Valiant's two-phase randomized routing.
    Valiant,
    /// Run both strategies (for sweep mode).
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
}
