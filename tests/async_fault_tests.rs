//! Integration tests for fault-tolerant routing (async engine).

#![cfg(feature = "async")]

use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use hypercube::engine::{AsyncEngine, DEFAULT_CHANNEL_CAPACITY, SimulationConfig};
use hypercube::routing::FaultTolerant;
use hypercube::topology::{FaultSet, Hypercube};
use hypercube::traffic::BitReversal;

fn config_with_faults(dim: u8, faults: FaultSet) -> SimulationConfig {
    SimulationConfig {
        dimension: dim,
        strategy: Arc::new(FaultTolerant),
        faults,
        channel_capacity: DEFAULT_CHANNEL_CAPACITY,
        seed: 42,
    }
}

#[tokio::test]
async fn no_faults_all_delivered() {
    let engine = AsyncEngine::build(config_with_faults(4, FaultSet::new()));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    assert_eq!(stats.delivered_packets, 16);
}

#[tokio::test]
async fn single_fault_4cube() {
    let mut faults = FaultSet::new();
    faults.fail_node(hypercube::topology::NodeId::new(5));

    let engine = AsyncEngine::build(config_with_faults(4, faults));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    assert!(
        stats.delivered_packets >= 14,
        "expected at least 14 delivered, got {}",
        stats.delivered_packets
    );
}

#[tokio::test]
async fn n_minus_1_faults_5cube() {
    let cube = Hypercube::new(5);
    let mut rng = ChaCha8Rng::seed_from_u64(99);
    let faults = FaultSet::random_node_faults(&cube, 4, &mut rng);

    let engine = AsyncEngine::build(config_with_faults(5, faults));
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    engine.inject(&BitReversal, &mut rng2);
    let stats = engine.run("bit-reversal", 200).await;

    assert!(
        stats.delivered_packets >= 24,
        "expected at least 24 delivered with 4 faults, got {}",
        stats.delivered_packets
    );
}

#[tokio::test]
async fn random_faults_6cube() {
    let cube = Hypercube::new(6);
    let mut rng = ChaCha8Rng::seed_from_u64(77);
    let faults = FaultSet::random_node_faults(&cube, 5, &mut rng);

    let engine = AsyncEngine::build(config_with_faults(6, faults));
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    engine.inject(&BitReversal, &mut rng2);
    let stats = engine.run("bit-reversal", 200).await;

    assert!(
        stats.delivered_packets >= 50,
        "expected at least 50 delivered with 5 faults on 6-cube, got {}",
        stats.delivered_packets
    );
}

#[tokio::test]
async fn link_faults() {
    let mut faults = FaultSet::new();
    faults.fail_link(
        hypercube::topology::NodeId::new(0),
        hypercube::topology::NodeId::new(1),
    );
    faults.fail_link(
        hypercube::topology::NodeId::new(2),
        hypercube::topology::NodeId::new(3),
    );
    faults.fail_link(
        hypercube::topology::NodeId::new(4),
        hypercube::topology::NodeId::new(5),
    );

    let engine = AsyncEngine::build(config_with_faults(4, faults));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    assert_eq!(stats.total_packets, 16);
    assert!(
        stats.delivered_packets >= 14,
        "expected most packets delivered with link faults, got {}",
        stats.delivered_packets
    );
}
