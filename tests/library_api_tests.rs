//! Tests for the library embedding API: `inject_one`, `drain_delivered`,
//! `tick_count`, `in_flight`, and generic payload support.

use std::sync::Arc;

use hypercube::engine::{Engine, SimulationConfig};
use hypercube::routing::{BitFixing, Valiant};
use hypercube::topology::{FaultSet, NodeId};

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
fn inject_one_and_deliver_unit_payload() {
    let mut engine: Engine<()> = Engine::build(config(3, Arc::new(BitFixing)));

    // Node 0 -> Node 0: should deliver in 0 hops on the first tick.
    let id = engine.inject_one(NodeId::new(0), NodeId::new(0), ());
    assert_eq!(engine.in_flight(), 1);

    engine.tick();
    let delivered = engine.drain_delivered();

    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].header.id, id);
    assert_eq!(delivered[0].header.source, NodeId::new(0));
    assert_eq!(delivered[0].header.destination, NodeId::new(0));
    assert_eq!(delivered[0].hops, 0);
    assert_eq!(engine.in_flight(), 0);
}

#[test]
fn inject_one_custom_payload() {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CoherenceMsg {
        kind: u8,
        addr: u64,
    }

    let mut engine: Engine<CoherenceMsg> = Engine::build(config(4, Arc::new(BitFixing)));

    let msg = CoherenceMsg {
        kind: 1,
        addr: 0xDEAD_BEEF,
    };

    let id = engine.inject_one(NodeId::new(0), NodeId::new(15), msg.clone());
    assert_eq!(engine.total_injected(), 1);
    assert_eq!(engine.in_flight(), 1);

    // Run until delivered.
    for _ in 0..20 {
        engine.tick();
    }

    let delivered = engine.drain_delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].header.id, id);
    assert_eq!(delivered[0].payload, msg);
    assert_eq!(delivered[0].header.destination, NodeId::new(15));
    assert_eq!(engine.in_flight(), 0);
}

#[test]
fn inject_one_with_valiant() {
    let mut engine: Engine<u32> = Engine::build(config(4, Arc::new(Valiant)));

    let id = engine.inject_one(NodeId::new(0), NodeId::new(15), 42);
    assert_eq!(engine.in_flight(), 1);

    for _ in 0..30 {
        engine.tick();
    }

    let delivered = engine.drain_delivered();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].header.id, id);
    assert_eq!(delivered[0].payload, 42);
    assert_eq!(engine.in_flight(), 0);
}

#[test]
fn drain_delivered_clears_buffer() {
    let mut engine: Engine<()> = Engine::build(config(3, Arc::new(BitFixing)));

    let _ = engine.inject_one(NodeId::new(0), NodeId::new(0), ());
    engine.tick();

    let first = engine.drain_delivered();
    assert_eq!(first.len(), 1);

    // Second drain should be empty.
    let second = engine.drain_delivered();
    assert!(second.is_empty());
}

#[test]
fn tick_count_tracks_ticks() {
    let mut engine: Engine<()> = Engine::build(config(3, Arc::new(BitFixing)));

    assert_eq!(engine.tick_count(), 0);
    engine.tick();
    assert_eq!(engine.tick_count(), 1);
    engine.tick();
    engine.tick();
    assert_eq!(engine.tick_count(), 3);
}

#[test]
fn in_flight_decrements_on_delivery() {
    let mut engine: Engine<()> = Engine::build(config(4, Arc::new(BitFixing)));

    // Inject 3 packets with varying distances.
    let _ = engine.inject_one(NodeId::new(0), NodeId::new(0), ()); // 0 hops
    let _ = engine.inject_one(NodeId::new(0), NodeId::new(1), ()); // 1 hop
    let _ = engine.inject_one(NodeId::new(0), NodeId::new(15), ()); // 4 hops

    assert_eq!(engine.in_flight(), 3);

    // Tick once: the self-send should deliver.
    engine.tick();
    let _ = engine.drain_delivered();
    assert_eq!(engine.in_flight(), 2);

    // Run until all delivered.
    for _ in 0..20 {
        engine.tick();
    }
    let _ = engine.drain_delivered();
    assert_eq!(engine.in_flight(), 0);
    assert_eq!(engine.total_delivered(), 3);
}

#[test]
fn multiple_packets_all_delivered() {
    let mut engine: Engine<u64> = Engine::build(config(4, Arc::new(BitFixing)));

    // Inject a packet from every node to its complement.
    for i in 0..16u16 {
        let _ = engine.inject_one(NodeId::new(i), NodeId::new(i ^ 0xF), i as u64);
    }

    assert_eq!(engine.total_injected(), 16);
    assert_eq!(engine.in_flight(), 16);

    for _ in 0..30 {
        engine.tick();
    }

    let delivered = engine.drain_delivered();
    assert_eq!(delivered.len(), 16);
    assert_eq!(engine.in_flight(), 0);

    // Verify all payloads arrived correctly.
    for pkt in &delivered {
        let expected_payload = pkt.header.source.raw() as u64;
        assert_eq!(pkt.payload, expected_payload);
        assert_eq!(
            pkt.header.destination,
            NodeId::new(pkt.header.source.raw() ^ 0xF)
        );
    }
}

#[test]
fn fencing_pattern() {
    // Simulate the pattern rvsim would use: inject, tick until in_flight == 0.
    let mut engine: Engine<&str> = Engine::build(config(4, Arc::new(BitFixing)));

    let _ = engine.inject_one(NodeId::new(3), NodeId::new(12), "invalidate");
    let _ = engine.inject_one(NodeId::new(7), NodeId::new(0), "fetch");

    while engine.in_flight() > 0 {
        engine.tick();
        let delivered = engine.drain_delivered();
        for pkt in &delivered {
            assert!(
                pkt.payload == "invalidate" || pkt.payload == "fetch",
                "unexpected payload"
            );
        }
    }

    assert_eq!(engine.total_delivered(), 2);
}

#[test]
fn inject_mid_simulation() {
    // Inject packets at different ticks (like a real cache controller would).
    let mut engine: Engine<u32> = Engine::build(config(4, Arc::new(BitFixing)));

    let _ = engine.inject_one(NodeId::new(0), NodeId::new(15), 100);
    engine.tick();
    engine.tick();

    // Inject a second packet mid-simulation.
    let _ = engine.inject_one(NodeId::new(5), NodeId::new(10), 200);
    assert_eq!(engine.in_flight(), 2);

    for _ in 0..30 {
        engine.tick();
    }

    let delivered = engine.drain_delivered();
    assert_eq!(delivered.len(), 2);

    let payloads: Vec<u32> = delivered.iter().map(|p| p.payload).collect();
    assert!(payloads.contains(&100));
    assert!(payloads.contains(&200));
}

#[test]
fn cube_accessor() {
    let engine: Engine<()> = Engine::build(config(5, Arc::new(BitFixing)));
    assert_eq!(engine.cube().dim(), 5);
    assert_eq!(engine.cube().node_count(), 32);
}
