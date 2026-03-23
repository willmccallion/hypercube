//! Integration tests for fault-tolerant routing.

use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use hypercube::engine::{Engine, SimulationConfig};
use hypercube::routing::FaultTolerant;
use hypercube::topology::{FaultSet, Hypercube};
use hypercube::traffic::BitReversal;

fn config_with_faults(dim: u8, faults: FaultSet) -> SimulationConfig {
    SimulationConfig {
        dimension: dim,
        strategy: Arc::new(FaultTolerant),
        faults,
        channel_capacity: 256,
        seed: 42,
    }
}

#[tokio::test]
async fn no_faults_all_delivered() {
    let engine = Engine::build(config_with_faults(4, FaultSet::new()));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    assert_eq!(stats.delivered_packets, 16);
}

#[tokio::test]
async fn single_fault_4cube() {
    let mut faults = FaultSet::new();
    faults.fail_node(hypercube::topology::NodeId::new(5));

    let engine = Engine::build(config_with_faults(4, faults));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    // 16 packets total. Packets sourced from or destined for the faulty node
    // may not deliver, but all others should.
    assert!(
        stats.delivered_packets >= 14,
        "expected at least 14 delivered, got {}",
        stats.delivered_packets
    );
}

#[tokio::test]
async fn n_minus_1_faults_5cube() {
    // A 5-cube is 5-connected. With 4 faults (n-1), the graph remains
    // connected. All surviving-node traffic should be deliverable.
    let cube = Hypercube::new(5);
    let mut rng = ChaCha8Rng::seed_from_u64(99);
    let faults = FaultSet::random_node_faults(&cube, 4, &mut rng);

    let engine = Engine::build(config_with_faults(5, faults));
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    engine.inject(&BitReversal, &mut rng2);
    let stats = engine.run("bit-reversal", 200).await;

    // With 4 faulted nodes out of 32, we lose packets involving those nodes.
    // But all packets between surviving nodes should deliver.
    // Minimum: 32 - 2*4 = 24 (worst case: each fault kills one src and one dst packet).
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
    // 5 faults on a 6-cube (n-1 = 5). Should remain connected.
    let faults = FaultSet::random_node_faults(&cube, 5, &mut rng);

    let engine = Engine::build(config_with_faults(6, faults));
    let mut rng2 = ChaCha8Rng::seed_from_u64(42);
    engine.inject(&BitReversal, &mut rng2);
    let stats = engine.run("bit-reversal", 200).await;

    // 64 packets, up to 10 might involve faulty nodes.
    assert!(
        stats.delivered_packets >= 50,
        "expected at least 50 delivered with 5 faults on 6-cube, got {}",
        stats.delivered_packets
    );
}

#[tokio::test]
async fn link_faults() {
    let mut faults = FaultSet::new();
    // Fail several links in a 4-cube.
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

    let engine = Engine::build(config_with_faults(4, faults));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100).await;

    // Link faults don't remove nodes, so all 16 packets should still deliver
    // (the graph is still connected with 3 link faults on a 4-cube).
    assert_eq!(stats.total_packets, 16);
    assert!(
        stats.delivered_packets >= 14,
        "expected most packets delivered with link faults, got {}",
        stats.delivered_packets
    );
}
