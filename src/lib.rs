//! Distributed hypercube routing simulator.
//!
//! Models an n-dimensional hypercube network with synchronous tick-based
//! simulation. The core engine is fully synchronous; an optional async
//! execution model is available behind the `async` feature flag.
//!
//! # Library usage
//!
//! For embedding as a network-on-chip transport (e.g., in a many-core
//! simulator), use [`Engine<T>`] with a custom payload type:
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use hypercube::engine::{DEFAULT_CHANNEL_CAPACITY, Engine, SimulationConfig};
//! use hypercube::routing::BitFixing;
//! use hypercube::topology::{FaultSet, NodeId};
//!
//! let config = SimulationConfig {
//!     dimension: 4,
//!     strategy: Arc::new(BitFixing),
//!     faults: FaultSet::new(),
//!     channel_capacity: DEFAULT_CHANNEL_CAPACITY,
//!     seed: 42,
//! };
//!
//! let mut net: Engine<u64> = Engine::build(config);
//! net.inject_one(NodeId::new(0), NodeId::new(15), 0xDEAD);
//! net.tick();
//!
//! for pkt in net.drain_delivered() {
//!     println!("delivered payload {} after {} hops", pkt.payload, pkt.hops);
//! }
//! ```

pub mod cli;
pub mod engine;
#[cfg(feature = "async")]
pub mod node;
pub mod packet;
pub mod routing;
pub mod stats;
pub mod topology;
pub mod traffic;
