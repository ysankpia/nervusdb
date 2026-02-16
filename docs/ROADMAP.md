# NervusDB Roadmap

> **Current Status**: SQLite-Beta convergence — TCK 100%, stability window in progress.

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

## Phase B: Stability Line — IN PROGRESS

**Goal**: 7 consecutive days of stable CI + nightly, no blocking failures.

| Milestone | Status |
|-----------|--------|
| Python exception hierarchy | Done |
| Node structured error payloads | Done |
| `storage_format_epoch` enforcement | Done |
| API freeze (Rust/CLI/Python/Node) | In progress |
| 7-day stability window | In progress (Day 1 = 2026-02-15) |

Stability window rules:
- Any blocking failure in main CI or nightly resets the counter.
- Nightly suite includes: TCK Tier-3, benchmark, chaos, soak, fuzz.

## Phase C: Performance Line — PLANNED

**Goal**: Large-scale SLO benchmarks pass before Beta release.

| Metric | Target |
|--------|--------|
| Read query P99 | <= 120 ms |
| Write transaction P99 | <= 180 ms |
| Vector search P99 | <= 220 ms |

Any SLO miss blocks Beta release.

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
2. 7 consecutive days stable CI + nightly — **in progress**.
3. Performance SLOs on large dataset — **planned**.

## Future (Post-Beta)

- crates.io / PyPI / npm publishing
- Performance benchmarks vs Neo4j / Memgraph
- Swift/iOS binding
- WebAssembly target
- Real-world case studies and examples
