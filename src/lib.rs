//! Distributed hypercube routing simulator.
//!
//! Models an n-dimensional hypercube network where each node runs as an
//! independent async task, implementing and comparing routing algorithms
//! including bit-fixing and Valiant's two-phase randomized routing.

pub mod packet;
pub mod routing;
pub mod topology;
pub mod traffic;
