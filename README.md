# Hypercube

A distributed hypercube routing simulator written in Rust. Simulates packet routing on n-dimensional hypercube networks and measures congestion, latency, and hop counts against theoretical bounds.

Each node runs as an independent async task (via Tokio), communicating through per-dimension channels with tick-synchronized barriers. The topology is implicit -- neighbors are computed via bit arithmetic, so no adjacency lists are stored.

## Why hypercubes?

An n-dimensional hypercube connects 2^n nodes where each node's neighbors differ by exactly one bit. This structure has been foundational in parallel computing theory since the 1980s:

- **Valiant and Brebner (1981)** proved that randomized two-phase routing achieves O(n) congestion on any permutation, vs. the Theta(sqrt(N)) worst-case of deterministic greedy routing.
- **The Connection Machine CM-2** (1987) was a physical 16-dimensional hypercube with 65,536 processors.
- Modern data center topologies (HyperX, fat-trees, dragonfly) trace their lineage to hypercube routing theory.

This simulator lets you run those classical experiments directly and verify the theoretical results.

## Quick start

```bash
# Single run: 10-cube, Valiant routing, bit-reversal traffic
cargo run --release -- --dim 10 --strategy valiant --traffic bit-reversal --max-ticks 80

# Sweep across dimensions and strategies, output to CSV
cargo run --release -- --sweep --dim-range 3:14 --strategy both --traffic bit-reversal -o results.csv

# Full parameter sweep: all traffic patterns, varying channel capacity
cargo run --release -- --sweep --dim-range 4:12 --strategy both \
    --traffic-range all --capacity-range "64:256:64" -o sweep.csv
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
8,256,valiant,bit-reversal,256,256,4,1.00,10,5.23,16,8.00,260;258;252;261;255;249;263;258,0;0;0;0;0;0;0;0
```

The `per_dim_load` and `per_dim_max_queue` columns are semicolon-delimited, one value per dimension. This lets you analyze congestion distribution across dimensions.

## Architecture

```
src/
  lib.rs                    Library root
  main.rs                   CLI binary
  cli.rs                    Argument parsing (clap)
  engine/
    mod.rs                  Simulation orchestration
    builder.rs              Channel and node assembly
  node/
    mod.rs                  Async node event loop
  packet.rs                 Packet structure and routing phases
  routing/
    mod.rs                  RoutingStrategy trait
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

**Key design decisions:**
- **Implicit topology**: No adjacency lists. Neighbors are computed via `node_id XOR (1 << dim)`. This uses O(1) space per node.
- **Tick-synchronous**: All nodes advance together via a shared barrier. This matches the BSP (Bulk Synchronous Parallel) model used in the theoretical analysis.
- **Per-node async tasks**: Each node is a `tokio::spawn` task with its own channel endpoints. This models the distributed nature of the system but limits scalability beyond ~16 dimensions.

## Performance

Measured on a single machine (release build):

| Dimension | Nodes | Approx. runtime |
|-----------|-------|-----------------|
| 8 | 256 | instant |
| 12 | 4,096 | < 1s |
| 14 | 16,384 | ~1s |
| 16 | 65,536 | ~17s |

## What to look for in results

**Validating Valiant's theorem**: Run bit-reversal traffic with `--strategy both`. Bit-fixing should show `max_congestion` growing as sqrt(N), while Valiant stays at O(n). At dim=14, bit-fixing congestion is ~128 vs. Valiant's ~8.

**Per-dimension load balancing**: In the CSV, check `per_dim_load`. Deterministic routing (bit-fixing) creates uneven load across dimensions. Valiant's randomization makes it nearly uniform.

**Hops vs. latency**: `max_hops` counts physical link traversals; `max_latency` counts ticks (which includes queuing delays). When congestion is low, they're similar. Under backpressure, latency exceeds hops.

## Related work

- [BookSim](https://github.com/booksim/booksim2) -- Stanford's interconnection network simulator. The standard tool in computer architecture research. Supports many topologies and routing algorithms.
- Valiant, L. G. (1982). "A scheme for fast parallel communication." *SIAM Journal on Computing*.
- Leighton, F. T. (1992). *Introduction to Parallel Algorithms and Architectures*. Morgan Kaufmann. Chapters 3-4 cover hypercube routing theory.

## Future: Multi-core interconnect for rvsim

This project is designed to eventually serve as the **network-on-chip (NoC) layer** for [rvsim](https://github.com/willmccallion/rvsim), a cycle-accurate RISC-V system simulator. The goal is to connect multiple rvsim cores through a hypercube interconnect, creating a full many-core simulator with realistic network contention modeling.

The integration path:

1. **Multi-core rvsim** — instantiate N cores with private L1 caches sharing a coherence domain
2. **Cache coherence protocol** — MESI/MOESI directory-based coherence, where invalidations, fetches, and writebacks become network messages
3. **Hypercube as the NoC transport** — coherence messages are packets routed through the hypercube. Miss latency depends on network congestion, hop count, and routing strategy — all things this simulator already measures
4. **Unified design-space exploration** — sweep over core count, cache sizes, network dimension, and routing strategy simultaneously to study the interplay between microarchitecture and interconnect

This mirrors the architecture of gem5's Ruby (coherence) + Garnet (network) subsystems, but built from scratch in Rust with both the CPU and network models under the same roof.

## License

MIT OR Apache-2.0
