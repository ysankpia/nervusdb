# NervusDB Roadmap

> **Current Status**: SQLite-Beta release line achieved on trunk — TCK 100%, stability 7/7, perf SLO window 7/7.

## Phase A: Feature Line — COMPLETE

**Goal**: openCypher TCK pass rate >= 95%.
**Result**: 100% (3 897 / 3 897 scenarios).

| Milestone | Status |
|-----------|--------|
| Tier-0 smoke gate | Done |
| Tier-1 clauses whitelist gate | Done |
| Tier-2 expressions whitelist gate | Done |
| Tier-3 full TCK nightly | Done |
| 95% threshold gate | Done (100%) |
| Failure clustering and batch fixes | Done |
| Three-platform binding alignment | Done |

## Phase B: Stability Line — COMPLETE

**Goal**: 7 consecutive days of stable CI + nightly, no blocking failures.

| Milestone | Status |
|-----------|--------|
| Python exception hierarchy | Done |
| Node structured error payloads | Done |
| `storage_format_epoch` enforcement | Done |
| API freeze (Rust/CLI/Python/Node) | Done |
| 7-day stability window | Done (7 / 7 reached on 2026-02-22 UTC) |

Stability window rules:
- Any blocking failure in main CI or nightly resets the counter.
- Nightly suite includes: TCK Tier-3, benchmark, chaos, soak, fuzz.

## Phase C: Performance Line — COMPLETE

**Goal**: Large-scale SLO benchmarks pass before Beta release.

| Metric | Target |
|--------|--------|
| Read query P99 | <= 120 ms |
| Write transaction P99 | <= 180 ms |
| Vector search P99 | <= 220 ms |

Any SLO miss blocks Beta release.

Current trunk status:
- P0 correctness/stability blockers cleared.
- Default HNSW params converged to `M=16`, `efConstruction=200`, `efSearch=128`.
- `perf-slo-nightly` is green on `main`.
- `perf_slo_window` reached **7 / 7** on `2026-03-26 (UTC)`.

## Industrial Quality (Continuous)

| Area | Status |
|------|--------|
| Fuzz (`cargo-fuzz` targets) | Active — parser/planner/executor |
| Chaos (IO fault injection) | Active — disk full, permission failures |
| Soak (24h stability) | Active — nightly/scheduled |
| WAL recovery verification | Done |

## Beta Release Criteria

All three must be met simultaneously:

1. TCK pass rate >= 95% — **achieved** (100%).
2. 7 consecutive days stable CI + nightly — **achieved**.
3. Performance SLOs on large dataset — **achieved**.

## Future (Post-Beta)

- crates.io / PyPI / npm publishing
- Performance benchmarks vs Neo4j / Memgraph
- Swift/iOS binding
- WebAssembly target
- Real-world case studies and examples
