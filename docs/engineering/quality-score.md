# Quality Score

## Assessment (0-5)

| Dimension | Score | Evidence |
|---|---|---|
| Product / Domain | 4 | Clear direction-contract, scope-0.1, vision, non-goals, user stories. Product bias documented. Ten user stories map to runnable examples. |
| Architecture | 4 | Crate boundaries documented. 5 workspace crates, clean separation. Storage/query/API models documented with invariants. Four ADRs record key decisions. |
| Validation | 4 | `scripts/check.sh` runs fmt + core clippy + 16 core tests. Validation-policy table maps change type to required check. Crash recovery script exists. CI runs core check on push/PR. |
| Documentation | 4 | All harness-layer docs exist and are coherent. Direction-contract, roadmap, architecture-invariants, quality-score, tech-debt all present. ADRs, plan template, bug template in place. |
| Maintainability | 4 | 5 focused crates, 6 scripts, 1 CI workflow. No bindings, no HNSW, no dead query code. Clean workspace. `.gitignore` covers OS/IDE/build/temp. |

## Dimension Details

### Product / Domain — 4

Strengths:

- `docs/product/vision.md` states "SQLite for property graphs" clearly.
- `docs/product/scope-0.1.md` lists concrete in-scope and out-of-scope items.
- `docs/product/non-goals.md` explicitly names frozen ambitions.
- `docs/product/user-stories-0.1.md` defines ten executable acceptance stories.

Gaps:

- No third-party validation that the ten user stories actually map to real use.

### Architecture — 4

Strengths:

- Crate boundaries documented in both `crate-boundaries.md` and `workspace-layers.md`.
- Core path: `nervusdb -> nervusdb-api -> nervusdb-storage / nervusdb-query -> nervusdb-cli`.
- Storage model documented with invariants (format epoch, WAL replay, reopen).
- Query model limited to Mini-Cypher with clear boundaries.
- API surface classified as core vs experimental/maintenance.
- ADR 0004 records the core/experimental/frozen layering decision.

Gaps:

- backup.rs, bulkload.rs, vacuum.rs still compiled in nervusdb-storage as dead code.
- No Cargo feature isolation for the storage-only dead code paths.

### Validation — 4

Strengths:

- `bash scripts/check.sh` is the fast default (fmt + core clippy + 16 core tests).
- `bash scripts/workspace_quick_test.sh` targets core Mini-Cypher only.
- Validation-policy table (`docs/engineering/validation-policy.md`) maps change type to required command.
- Crash recovery script (`scripts/core_crash_recovery.sh`) exists and passes.
- CI (`ci.yml`) runs `bash scripts/check.sh` on push/PR to main.

Gaps:

- No large-scale acceptance test (1M nodes / 5M edges).
- No benchmark regression detection.
- No documented regression guard requirement for bug fixes in script form.

### Documentation — 4

Strengths:

- `docs/index.md` is the default map.
- Product, architecture, and engineering docs are present and coherent.
- ADRs record key decisions with context and consequences.
- Plan template and bug template exist.

Gaps:

- Rust public API has near-zero rustdoc comments.
- README quickstart code not verified as compilable.
- Some engineering docs overlap (branching-pr.md could merge into git-workflow.md).

### Maintainability — 4

Strengths:

- Workspace shrunk to 5 crates (removed pyo3, capi).
- All 6 kept scripts are core-focused and documented in runbooks.
- Single CI workflow (ci.yml), no nightly noise.
- No bindings, no HNSW, no historical integration tests in workspace.
- `.gitignore` covers OS, IDE, build, temp, and generated artifacts.

Gaps:

- backup.rs, bulkload.rs, vacuum.rs (1,503 lines) still compile as dead code.
- Plan variants MatchIn, MatchUndirected, IndexSeek, Apply, ProcedureCall, Foreach still exist in source (return error at runtime).
- Merge-related fields in PreparedQuery never read.
