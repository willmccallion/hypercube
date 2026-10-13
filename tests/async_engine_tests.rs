//! Integration tests for the async simulation engine.
//!
//! These mirror the sync engine tests in `engine_tests.rs` but use the
//! Tokio-based [`AsyncEngine`] to verify both execution models produce
//! correct results.

#![cfg(feature = "async")]

use std::sync::Arc;

use hypercube::engine::{AsyncEngine, DEFAULT_CHANNEL_CAPACITY, SimulationConfig};
use hypercube::routing::{BitFixing, Valiant};
use hypercube::topology::FaultSet;
use hypercube::traffic::{BitReversal, Complement, Identity, RandomPermutation};

fn config(dim: u8, strategy: Arc<dyn hypercube::routing::RoutingStrategy>) -> SimulationConfig {
    SimulationConfig {
        dimension: dim,
        strategy,
        faults: FaultSet::new(),
        channel_capacity: DEFAULT_CHANNEL_CAPACITY,
        seed: 42,
    }
}

#[tokio::test]
async fn identity_bitfix_all_delivered() {
    let engine = AsyncEngine::build(config(3, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&Identity, &mut rng);
    let stats = engine.run("identity", 50).await;

    assert_eq!(stats.total_packets, 8);
    assert_eq!(stats.delivered_packets, 8);
    assert_eq!(stats.max_latency, 0);
}

#[tokio::test]
async fn bitfix_single_packet_3cube() {
    let engine = AsyncEngine::build(config(3, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&Identity, &mut rng);
    let stats = engine.run("identity", 50).await;

    assert_eq!(stats.delivered_packets, 8);
}

#[tokio::test]
async fn bit_reversal_bitfix_all_delivered() {
    let engine = AsyncEngine::build(config(4, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    assert_eq!(stats.total_packets, 16);
    assert_eq!(stats.delivered_packets, 16);
}

#[tokio::test]
async fn complement_bitfix_all_delivered() {
    let engine = AsyncEngine::build(config(4, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&Complement, &mut rng);
    let stats = engine.run("complement", 100).await;

    assert_eq!(stats.total_packets, 16);
    assert_eq!(stats.delivered_packets, 16);
}

#[tokio::test]
async fn bit_reversal_valiant_all_delivered() {
    let engine = AsyncEngine::build(config(4, Arc::new(Valiant)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    assert_eq!(stats.total_packets, 16);
    assert_eq!(stats.delivered_packets, 16);
}

#[tokio::test]
async fn random_perm_valiant_6cube() {
    let engine = AsyncEngine::build(config(6, Arc::new(Valiant)));
    let mut rng = rand::rng();
    engine.inject(&RandomPermutation, &mut rng);
    let stats = engine.run("random", 100).await;

    assert_eq!(stats.total_packets, 64);
    assert_eq!(stats.delivered_packets, 64);
}

#[tokio::test]
async fn random_perm_bitfix_6cube() {
    let engine = AsyncEngine::build(config(6, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&RandomPermutation, &mut rng);
    let stats = engine.run("random", 100).await;

    assert_eq!(stats.total_packets, 64);
    assert_eq!(stats.delivered_packets, 64);
}

#[tokio::test]
async fn larger_cube_8dim() {
    let engine = AsyncEngine::build(config(8, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 200).await;

    assert_eq!(stats.total_packets, 256);
    assert_eq!(stats.delivered_packets, 256);
}

#[tokio::test]
async fn valiant_larger_cube_8dim() {
    let engine = AsyncEngine::build(config(8, Arc::new(Valiant)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 200).await;

    assert_eq!(stats.total_packets, 256);
    assert_eq!(stats.delivered_packets, 256);
}
