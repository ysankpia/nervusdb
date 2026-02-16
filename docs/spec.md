# NervusDB v2 — Technical Specification (v2.3, SQLite-Beta Convergence)

> This spec is the engineering constitution for NervusDB v2.
> The goal is not "looks done" — it is reaching the Beta release line through
> repeatable, evidence-based quality gates.

## 1. Project Positioning

- **Mission**: A pure-Rust embedded property graph database with SQLite-style
  "open a path and go" ergonomics.
- **Core path**: open DB -> write -> query (including streaming) -> crash recovery
  -> consistent cross-language behavior.

## 2. Scope and Release Strategy (Locked)

- Scope: single-machine embedded only (Rust + CLI + Python + Node.js).
- Out of scope: remote server, distributed mode, migration compatibility promises.
- Allowed: breaking changes during Beta convergence, but storage format epoch must
  be explicitly versioned.

## 3. Beta Hard Gates (All Must Be Met Simultaneously)

1. Official openCypher TCK full pass rate **>=95%** (Tier-3 full scope).
   Current status: **100% (3 897 / 3 897)** — achieved.
2. Warnings treated as blocking (`fmt` / `clippy` / `tests` / `bindings` chain).
3. Freeze phase: **7 consecutive days** of stable main CI + nightly.
   Current status: stability window in progress (Day 1 = 2025-02-15).

> If any gate is not met, the project is considered "not yet at SQLite-for-graphs
> (Beta) level".

## 4. Storage Compatibility and Error Model

- `storage_format_epoch` is introduced and enforced.
- Epoch mismatch returns `StorageFormatMismatch` (Compatibility semantics).
- Unified error categories: `Syntax / Execution / Storage / Compatibility`.

Cross-language error mapping:

| Platform | Error Types |
|----------|-------------|
| Python | `NervusError`, `SyntaxError`, `ExecutionError`, `StorageError`, `CompatibilityError` |
| Node.js | Structured error payload: `{ code, category, message }` |

## 5. Quality Gate Matrix

### 5.1 PR-Blocking Gates

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
3. `bash scripts/workspace_quick_test.sh`
4. `bash scripts/tck_tier_gate.sh tier0|tier1|tier2`
5. `bash scripts/binding_smoke.sh && bash scripts/contract_smoke.sh`

### 5.2 Nightly / Manual Gates

1. Tier-3 full run + failure clustering + pass-rate report (`scripts/tck_full_rate.sh`)
2. Beta threshold gate (`scripts/beta_gate.sh`, default 95%)
3. Benchmark / chaos / soak / fuzz

## 6. Test Coverage Requirements

| Suite | Tests | Target | Current |
|-------|-------|--------|---------|
| openCypher TCK | 3 897 | >=95% | 100% |
| Rust unit + integration | 153 | all green | all green |
| Python (PyO3) | 138 | all green | all green |
| Node.js (N-API) | 109 | all green | all green |

## 7. Execution Cadence

- **Phase A**: TCK feature line (reach 95%) — **COMPLETE** (100% achieved).
- **Phase B**: Stability freeze (7-day stability window) — **IN PROGRESS**.
- **Phase C**: Performance seal (large-scale SLO) — PLANNED.

## 8. Single Source of Truth

| Document | Path | Purpose |
|----------|------|---------|
| Specification | `docs/spec.md` | Engineering constitution (this file) |
| Tasks | `docs/tasks.md` | Progress tracking |
| Roadmap | `docs/ROADMAP.md` | Phase planning |
| Architecture | `docs/architecture.md` | System design |
| Cypher Support | `docs/cypher-support.md` | Compliance matrix |
| User Guide | `docs/user-guide.md` | API reference |

If documents conflict, **code + CI/nightly gate results + current task status**
take precedence. Fix the document immediately.
