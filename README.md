# Hypercube

Simulates packet routing on n-dimensional hypercube networks. Measures congestion, latency, and hop counts for different routing strategies and traffic patterns.

The engine is tick-synchronous -- a single `tick()` call advances all nodes using double-buffered link queues. There's also an async mode behind the `async` feature flag that spawns per-node Tokio tasks. Neighbors are computed via bit-flips (`node_id XOR (1 << dim)`), so no adjacency lists are stored.

## Background

An n-dimensional hypercube connects 2^n nodes where each node's neighbors differ by exactly one bit. Valiant and Brebner (1981) showed that randomized two-phase routing achieves O(n) congestion on any permutation, compared to the Theta(sqrt(N)) worst-case of deterministic greedy routing. This simulator runs those experiments and checks the results against the theory.

## Quick start

```bash
# Single run: 10-cube, Valiant routing, bit-reversal traffic
cargo run --release -- --dim 10 --strategy valiant --traffic bit-reversal --max-ticks 80

# Sweep across dimensions and strategies, output to CSV
cargo run --release -- --sweep --dim-range 3:14 --strategy both --traffic bit-reversal -o results.csv

# Full parameter sweep: all traffic patterns, varying channel capacity
cargo run --release -- --sweep --dim-range 4:12 --strategy both \
    --traffic-range all --capacity-range "64:256:64" -o sweep.csv

# Generate charts from sweep data
python3 -m venv .venv && .venv/bin/pip install matplotlib pandas
.venv/bin/python scripts/plot.py sweep.csv
```

## Routing strategies

| Strategy | Flag | Description | Worst-case congestion |
|----------|------|-------------|----------------------|
| **Bit-fixing** | `--strategy bitfix` | Greedy: fix differing bits MSB-first | Theta(sqrt(N)) |
| **Valiant** | `--strategy valiant` | Two-phase: route to random intermediate, then to destination | O(n) w.h.p. |
| **Fault-tolerant** | `--strategy fault-tolerant` | Misroutes around faulty nodes/links | Depends on fault count |
| **Both** | `--strategy both` | Runs bit-fixing and Valiant (for sweep comparisons) | -- |

## Traffic patterns

| Pattern | Flag | Description |
|---------|------|-------------|
| **Bit-reversal** | `--traffic bit-reversal` | Node x sends to reverse(x). Classic adversarial pattern. |
| **Complement** | `--traffic complement` | Node x sends to NOT(x). Every packet traverses all n dimensions. |
| **Transpose** | `--traffic transpose` | Swaps upper and lower address halves. |
| **Butterfly** | `--traffic butterfly` | Swaps bit 0 and bit n-1. Models FFT cross-wiring. |
| **Shuffle** | `--traffic shuffle` | Circular left-shift of address bits. Models shuffle-exchange networks. |
| **Random** | `--traffic random` | Uniform random permutation (Fisher-Yates). |
| **Identity** | `--traffic identity` | Each node sends to itself. Zero-hop baseline. |

## CLI reference

### Single run

```
cargo run --release -- [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--dim` | 6 | Hypercube dimension (nodes = 2^dim) |
| `--strategy` | valiant | Routing strategy |
| `--traffic` | bit-reversal | Traffic pattern |
| `--max-ticks` | 100 | Simulation duration in ticks |
| `--seed` | 42 | RNG seed |
| `--channel-capacity` | 256 | Buffer size per inter-node link |
| `--faults` | 0 | Number of random node faults to inject |
| `-o, --output` | stdout | CSV output file path |

### Sweep mode

```
cargo run --release -- --sweep [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--dim-range` | 3:12 | Dimension range (lo:hi inclusive) |
| `--strategy` | valiant | Strategy (use `both` to compare) |
| `--traffic-range` | (uses --traffic) | `"all"` or comma-separated list |
| `--capacity-range` | (uses --channel-capacity) | `start:end:step` format |
| `--runs` | 1 | Runs per configuration (vary seed) |

## CSV output

Each row is one simulation run:

```
dimension,nodes,strategy,traffic,total_packets,delivered,max_congestion,avg_congestion,max_latency,avg_latency,max_hops,avg_hops,per_dim_load,per_dim_max_queue
8,256,valiant,bit-reversal,256,256,4,1.02,9,4.50,14,8.13,266;282;268;256;252;254;254;250,0;0;0;0;0;0;0;0
```

The `per_dim_load` and `per_dim_max_queue` columns are semicolon-delimited, one value per dimension. This lets you analyze congestion distribution across dimensions.

## Architecture

```
src/
  lib.rs                    Library root, crate docs with embedding example
  main.rs                   CLI binary (sync, no Tokio runtime)
  cli.rs                    Argument parsing (clap)
  engine/
    mod.rs                  Sync Engine<T> with tick(), inject_one(), drain_delivered()
    builder.rs              SimulationConfig and engine construction
    async_engine.rs         AsyncEngine (behind "async" feature)
  node/
    mod.rs                  Per-node Tokio task (behind "async" feature)
  packet.rs                 PacketHeader + Packet<T> with Deref
  routing/
    mod.rs                  RoutingStrategy trait (operates on PacketHeader)
    bitfix.rs               Bit-fixing (greedy) routing
    valiant.rs              Valiant's two-phase routing
    fault_tolerant.rs       Fault-tolerant routing with misrouting
  topology/
    mod.rs                  Hypercube structure and neighbor queries
    address.rs              NodeId and bitwise operations
    faults.rs               Node and link fault tracking
  traffic/
    mod.rs                  TrafficPattern trait
    permutations.rs         Deterministic permutation patterns
    random.rs               Random permutation (Fisher-Yates)
    hotspot.rs              Hot-spot traffic pattern
  stats/
    mod.rs                  Statistics module
    metrics.rs              NodeStats and SimulationStats
    export.rs               CSV output
```

Notes:
- Neighbors are implicit: `node_id XOR (1 << dim)`, no adjacency lists.
- Double-buffered `VecDeque` link queues prevent packets from advancing more than one hop per tick (BSP model).
- `Engine<T>` is sync with no Tokio dependency. `AsyncEngine` (behind `async` feature) spawns per-node Tokio tasks.
- `Packet<T>` carries an arbitrary payload. `RoutingStrategy` operates on `PacketHeader` only, so the trait is non-generic and object-safe.

## Library usage

`Engine<T>` is generic over payload type. Without the `async` feature, there's no Tokio dependency.

```toml
[dependencies]
hypercube = { path = "../hypercube" }
```

```rust
use std::sync::Arc;
use hypercube::engine::{Engine, SimulationConfig};
use hypercube::routing::BitFixing;
use hypercube::topology::{FaultSet, NodeId};

let config = SimulationConfig {
    dimension: 4,
    strategy: Arc::new(BitFixing),
    faults: FaultSet::new(),
    channel_capacity: 256,
    seed: 42,
};

let mut net: Engine<u64> = Engine::build(config);

// Inject a message from core 0 to core 15.
net.inject_one(NodeId::new(0), NodeId::new(15), 0xDEAD);

// Step the network and collect delivered messages.
while net.in_flight() > 0 {
    net.tick();
    for pkt in net.drain_delivered() {
        println!("delivered payload {} after {} hops", pkt.payload, pkt.hops);
    }
}
```

## Performance

Measured on a single machine (release build):

| Dimension | Nodes | Approx. runtime |
|-----------|-------|-----------------|
| 8 | 256 | instant |
| 12 | 4,096 | < 1s |
| 14 | 16,384 | ~1s |
| 16 | 65,536 | ~17s |

## Results

Generated from a full sweep across dimensions 3--16 (8 to 65,536 nodes), 6 traffic patterns, both routing strategies, 3 runs per configuration (504 simulations):

```bash
cargo run --release -- --sweep --dim-range 3:16 --strategy both \
    --traffic-range "bit-reversal,complement,transpose,butterfly,shuffle,random" \
    --runs 3 -o full_study.csv
.venv/bin/python scripts/plot.py full_study.csv
```

### Congestion scaling by traffic pattern

![Congestion scaling](charts/congestion_scaling.png)

Three regimes show up. Adversarial patterns (bit-reversal, transpose): bit-fixing congestion tracks sqrt(N) while Valiant stays at O(n), crossing over around dim 5--7. Easy patterns (butterfly, complement): bit-fixing gets congestion 1, and Valiant actually makes it worse by adding random intermediates. Middle ground (random, shuffle): bit-fixing stays low, Valiant's overhead dominates.

### Strategy comparison

![Strategy comparison](charts/strategy_comparison.png)

Left panel (greedy): congestion varies widely -- bit-reversal/transpose hit 128 at dim=16, while complement/butterfly sit at 1. Right panel (Valiant): all patterns fall into a tight O(n) band regardless of traffic, but it's never optimal on easy patterns.

### Latency vs. hops

![Latency vs hops](charts/latency_vs_hops.png)

Packets move ~2 hops per tick since all nodes forward simultaneously. Latency scales linearly with hop count. Bit-fixing shows separation: complement (16 hops at dim=16) has the highest latency, butterfly (avg 1 hop, max 2) the lowest. Valiant collapses patterns into a tight band since the random intermediate dominates path length.

### Hop distribution

![Hop distribution](charts/hop_distribution.png)

Left panel (greedy): complement traverses all n dimensions. Butterfly averages 1 hop (swaps bits 0 and n-1; packets needing both flips take 2). The rest cluster around n/2. Right panel (Valiant): the random intermediate roughly doubles path length, pushing everything toward n hops.

## Theory notes

Observations from the simulation data.

### Why sqrt(N) congestion?

Consider bit-reversal on a 16-cube. Node `0000000011111111` sends to `1111111100000000`. Under MSB-first bit-fixing, the packet flips bit 15, then 14, ..., then 0. All packets whose source and destination differ on bit k cross the same dimension-k edges in the same direction. For bit-reversal, bit k is flipped whenever source bit k differs from source bit (n-1-k). At the middle dimensions (around k = n/2), many packets funnel through the same edge, creating the Theta(sqrt(N)) bottleneck.

The data shows max_congestion tracks 2^(floor((d-2)/2)) for dimensions 3--16: 1, 2, 2, 4, 4, 8, 8, 16, 16, 32, 32, 64, 64, 128. For even n that's sqrt(N)/2; for odd n it's sqrt(N)/(2*sqrt(2)).

### Why complement has congestion 1

The complement permutation (node x sends to NOT(x)) flips every bit. Under MSB-first bit-fixing, every packet traverses all n dimensions. The total per-dimension load is N (every packet crosses every dimension). Each dimension has N directed edges (one per node), so the average per-edge load is N/N = 1. In fact the maximum is also 1: complement is a perfect matching on every dimension cut, so no directed edge carries more than one packet. This gives max_congestion = 1 regardless of dimension.

### The Valiant tradeoff in numbers

Valiant sends each packet to a random intermediate before routing to the destination. This gives O(n) congestion on any permutation -- the data shows max_congestion of 8--9 at dim=16 across all six traffic patterns.

The cost is path length. Bit-fixing on bit-reversal averages floor(n/2) hops; Valiant averages ~n (n/2 per phase, two phases). For butterfly (avg 1 hop with bit-fixing), Valiant inflates to ~n hops at dim=16 for no congestion benefit.

The crossover is around dim 6--8 for adversarial patterns. At dim=6 for bit-reversal, bit-fixing congestion is 4 and Valiant varies 3--5 across runs, so it's a wash. By dim=8, Valiant reliably wins (4--6 vs. 8).

### Odd-dimension fixed points

For odd n, the bit-reversal permutation has a fixed point at the middle bit (bit floor(n/2)): reversing the bit positions maps the middle bit to itself. This means no packet ever needs to flip that dimension, so its per-dimension load is exactly 0. You can see this in the per_dim_load column -- odd dimensions have one zero entry. This is also why avg_hops = floor(n/2) rather than n/2 for bit-reversal.

### Shuffle has congestion 2

The perfect shuffle (circular left-shift) averages n/2 hops per packet. The data shows max_congestion = 2 for bit-fixing across all dimensions 3--16. The per-dimension load is exactly N/2 for every dimension -- the rotation distributes traffic evenly. With N/2 packets spread across N/2 directed edges per dimension, the average per-edge load is 1 and the max is 2.

## Related work

- [BookSim](https://github.com/booksim/booksim2) -- Stanford's interconnection network simulator. Supports many topologies and routing algorithms.
- Valiant, L. G. (1982). "A scheme for fast parallel communication." *SIAM Journal on Computing*.
- Leighton, F. T. (1992). *Introduction to Parallel Algorithms and Architectures*. Morgan Kaufmann. Chapters 3-4 cover hypercube routing theory.

## Future

The plan is to use this as the interconnect for [rvsim](https://github.com/willmccallion/rvsim), a RISC-V simulator. `Engine<T>` can carry coherence messages (`Packet<CoherenceMsg>`) between cores using `inject_one`/`tick`/`drain_delivered`.

What's left:

1. Multi-core rvsim with private L1 caches
2. Cache coherence protocol (MESI/MOESI), with invalidations/fetches/writebacks as packet payloads
3. Wire up the transport -- cache controllers call `inject_one` and poll `drain_delivered` each cycle
4. Sweep over core count, cache sizes, dimension, and routing strategy together

## License

MIT OR Apache-2.0
