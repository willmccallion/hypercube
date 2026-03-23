//! Distributed hypercube routing simulator.
//!
//! Models an n-dimensional hypercube network with synchronous tick-based
//! simulation. The core engine is fully synchronous; an optional async
//! execution model is available behind the `async` feature flag.

pub mod cli;
pub mod engine;
#[cfg(feature = "async")]
pub mod node;
pub mod packet;
pub mod routing;
pub mod stats;
pub mod topology;
pub mod traffic;
