# Quality Score

## Assessment (0-5)

| Dimension | Score | Evidence |
|---|---|---|
| Product / Domain | 4 | Clear direction-contract, scope-0.1, vision, non-goals, user stories. Product bias documented. Ten user stories map to runnable examples. |
| Architecture | 4 | Crate boundaries documented (core/experimental/frozen). Workspace layers classified. Storage/query/API models documented with invariants. Four ADRs record key decisions. |
| Validation | 4 | `scripts/check.sh` runs fmt + core clippy + quick test. Validation-policy table maps change type to required check. Testing strategy separates core from historical. Crash recovery script exists. CI runs core check on push/PR. |
| Documentation | 3 | All harness-layer docs exist. Missing: direction-contract, roadmap, PROGRESS, quality-score, architecture-invariants, git-workflow, dependency-policy, tech-debt, doc-gardening, local-setup, generated-artifacts. Some cross-references between docs are implicit. |
| Maintainability | 3 | Core crates are focused (nervusdb-storage, nervusdb-query, nervusdb, nervusdb-api, nervusdb-cli). Experimental/frozen code lives alongside core without Cargo feature isolation. `scripts/` has 36 entries — many are historical. `.github/workflows/` has 11 workflows — only `ci.yml` is default. |

## Dimension Details

### Product / Domain — 4

Strengths:

- `docs/product/vision.md` states "SQLite for property graphs" clearly.
- `docs/product/scope-0.1.md` lists concrete in-scope and out-of-scope items.
- `docs/product/non-goals.md` explicitly names frozen ambitions.
- `docs/product/user-stories-0.1.md` defines ten executable acceptance stories.

Gaps:

- No direction-contract.md was present before this backfill (now created).
- No roadmap.md was present before this backfill (now created).
- No PROGRESS.md was present before this backfill (now created).

### Architecture — 4

Strengths:

- Crate boundaries documented in both `crate-boundaries.md` and `workspace-layers.md`.
- Core path: `nervusdb -> nervusdb-api -> nervusdb-storage / nervusdb-query -> nervusdb-cli`.
- Storage model documented with invariants (format epoch, WAL replay, reopen).
- Query model limited to Mini-Cypher with clear boundaries.
- API surface classified as core vs experimental/maintenance.
- ADR 0004 records the core/experimental/frozen layering decision.

Gaps:

- `nervusdb-pyo3`, `nervusdb-node`, `nervusdb-capi` remain workspace members without Cargo feature isolation.
- HNSW/vector code lives inside `nervusdb-storage` with no feature gate.
- No architecture-invariants.md existed before this backfill (now created).

### Validation — 4

Strengths:

- `bash scripts/check.sh` is the fast default (fmt + core clippy + quick test).
- `bash scripts/workspace_quick_test.sh` targets core Mini-Cypher only.
- Validation-policy table (`docs/engineering/validation-policy.md`) maps change type to required command.
- Testing strategy (`docs/engineering/testing-strategy.md`) documents cost rule and required test bias.
- Crash recovery script (`scripts/core_crash_recovery.sh`) exists and passes.
- CI (`ci.yml`) runs `bash scripts/check.sh` on push/PR to main.

Gaps:

- 36 scripts in `scripts/` — majority are historical and not default-loop.
- 11 workflows in `.github/workflows/` — 10 are nightly/manual.
- No workspace_full_test.sh envelope to prove the fast path does not hide failures.
- No documented regression guard requirement for bug fixes in script form.

### Documentation — 3

Strengths:

- `docs/index.md` is the default map.
- Product, architecture, and engineering docs are present and coherent.
- ADRs record key decisions with context and consequences.
- Plan template and bug template exist.

Gaps (addressed by this backfill):

- Missing: direction-contract.md, roadmap.md, PROGRESS.md, quality-score.md, architecture-invariants.md, git-workflow.md, dependency-policy.md, tech-debt.md, doc-gardening.md, local-setup.md, generated-artifacts.md.
- README quick start does not reference the direction-contract or roadmap.
- Some engineering docs overlap (branching-pr.md could merge into git-workflow.md).

### Maintainability — 3

Strengths:

- Workspace crate count is small (7 crates).
- Core crates are well-separated by responsibility.
- `.gitignore` covers OS, IDE, build, temp, and generated artifacts.

Gaps:

- 36 scripts with undocumented historical purposes.
- 11 CI workflows — only `ci.yml` is default; others add noise.
- Experimental bindings (pyo3, node, capi) are workspace members, adding build surface.
- No Cargo feature isolation for experimental or frozen code paths.
- `scripts/` has no index or responsible-owner annotations.
