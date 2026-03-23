//! Integration tests for the simulation engine.

use std::sync::Arc;

use hypercube::engine::{Engine, SimulationConfig};
use hypercube::routing::{BitFixing, Valiant};
use hypercube::topology::FaultSet;
use hypercube::traffic::{BitReversal, Complement, Identity, RandomPermutation};

fn config(dim: u8, strategy: Arc<dyn hypercube::routing::RoutingStrategy>) -> SimulationConfig {
    SimulationConfig {
        dimension: dim,
        strategy,
        faults: FaultSet::new(),
        channel_capacity: 256,
        seed: 42,
    }
}

#[test]
fn identity_bitfix_all_delivered() {
    let mut engine = Engine::build(config(3, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&Identity, &mut rng);
    let stats = engine.run("identity", 50);

    assert_eq!(stats.total_packets, 8);
    assert_eq!(stats.delivered_packets, 8);
    assert_eq!(stats.max_latency, 0);
}

#[test]
fn bitfix_single_packet_3cube() {
    // In a 3-cube, packet from 0 to 7 should take 3 hops.
    let mut engine = Engine::build(config(3, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&Identity, &mut rng);
    let stats = engine.run("identity", 50);

    // Identity: all packets go to themselves, so 0 hops.
    assert_eq!(stats.delivered_packets, 8);
}

#[test]
fn bit_reversal_bitfix_all_delivered() {
    let mut engine = Engine::build(config(4, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100);

    assert_eq!(stats.total_packets, 16);
    assert_eq!(stats.delivered_packets, 16);
}

#[test]
fn complement_bitfix_all_delivered() {
    let mut engine = Engine::build(config(4, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&Complement, &mut rng);
    let stats = engine.run("complement", 100);

    assert_eq!(stats.total_packets, 16);
    assert_eq!(stats.delivered_packets, 16);
}

#[test]
fn bit_reversal_valiant_all_delivered() {
    let mut engine = Engine::build(config(4, Arc::new(Valiant)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 100);

    assert_eq!(stats.total_packets, 16);
    assert_eq!(stats.delivered_packets, 16);
}

#[test]
fn random_perm_valiant_6cube() {
    let mut engine = Engine::build(config(6, Arc::new(Valiant)));
    let mut rng = rand::rng();
    engine.inject(&RandomPermutation, &mut rng);
    let stats = engine.run("random", 100);

    assert_eq!(stats.total_packets, 64);
    assert_eq!(stats.delivered_packets, 64);
}

#[test]
fn random_perm_bitfix_6cube() {
    let mut engine = Engine::build(config(6, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&RandomPermutation, &mut rng);
    let stats = engine.run("random", 100);

    assert_eq!(stats.total_packets, 64);
    assert_eq!(stats.delivered_packets, 64);
}

#[test]
fn larger_cube_8dim() {
    // 256 nodes — verify no deadlocks or lost packets.
    let mut engine = Engine::build(config(8, Arc::new(BitFixing)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 200);

    assert_eq!(stats.total_packets, 256);
    assert_eq!(stats.delivered_packets, 256);
}

#[test]
fn valiant_larger_cube_8dim() {
    let mut engine = Engine::build(config(8, Arc::new(Valiant)));
    let mut rng = rand::rng();
    engine.inject(&BitReversal, &mut rng);
    let stats = engine.run("bit-reversal", 200);

    assert_eq!(stats.total_packets, 256);
    assert_eq!(stats.delivered_packets, 256);
}
