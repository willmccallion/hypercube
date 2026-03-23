//! Statistics collection and aggregation for simulation runs.

mod export;
mod metrics;

pub use export::write_csv;
pub use metrics::{NodeStats, SimulationStats};
