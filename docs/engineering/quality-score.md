# Quality Score

## Assessment (0-5)

| Dimension | Score | Evidence |
|---|---|---|
| Product / Domain | 4 | Direction, scope, non-goals, and active Fjall plan now define a finishable embedded Rust graph core. |
| Architecture | 4 | Code now matches the Fjall directory-storage contract; query/storage meet through `nervusdb-api`; old storage code was deleted. |
| Validation | 4 | Fjall reopen, label scan, traversal, property, snapshot, crash recovery, examples, default check, and workspace tests passed. |
| Documentation | 4 | Current docs now name the storage reset and scope boundaries directly. |
| Maintainability | 4 | The storage crate is reduced to Fjall glue plus graph semantics; false index and compaction hooks were removed from the public API. |

## Dimension Details

### Product / Domain — 4

Strengths:

- `docs/product/vision.md` states "SQLite for property graphs" clearly.
- `docs/product/scope-0.1.md` limits the product to embedded Rust graph core.
- `docs/product/non-goals.md` blocks platform and full-Cypher scope creep.
- ADR 0005 states why Fjall replaces self-built storage.

Gaps:

- No third-party validation that the ten user stories map to real use.
- Release-scale smoke remains manual; current checked smoke is 10k nodes and
  50k unique edges.

### Architecture — 4

Strengths:

- Crate boundaries now state that query and storage meet only through
  `nervusdb-api`.
- Storage model now describes logical Fjall keyspaces.
- API surface now removes `.ndb/.wal` from 0.1 core.
- `nervusdb-query` no longer depends on `nervusdb-storage`.
- `nervusdb-storage` no longer contains Pager/WAL/B+Tree/CSR/read-path modules.
- Labels and relationship types are separate keyspaces and counters.

Gaps:

- Large release-scale storage evidence is still manual rather than part of the
  default validation path.

### Validation — 4

Strengths:

- `bash scripts/check.sh` remains the default validation path.
- Crash recovery script exists.
- Validation policy now includes Fjall-specific focused checks.
- `cargo test --workspace` passes after the Fjall refactor.
- `bash scripts/core_crash_recovery.sh` passes against the Fjall backend.

Gaps:

- Large acceptance runs remain manual and should be recorded for release
  candidates.

### Documentation — 4

Strengths:

- ADR 0005 and active plan 010 are current direction.
- Product, architecture, API, storage, validation, and debt docs share the same
  storage model.

Gaps:

- Some archived and older reference material may still mention the old storage
  model. It is not current unless linked from `docs/index.md`.

### Maintainability — 4

Strengths:

- Workspace has 5 focused crates.
- Current docs forbid restoring platform-era breadth.
- Fjall reduces the amount of custom storage code NervusDB must own.
- Storage crate currently has only API re-export, error, property, snapshot,
  engine, and crash-test surfaces.

Gaps:

- Existing advanced query code remains outside the intended 0.1 core and should
  not be promoted by tests without a future ADR.
