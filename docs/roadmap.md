# Roadmap

## Current Phase

NervusDB 0.0.7 is in release preparation. The release is about storage epoch 3,
clean reopen, and footprint reduction, not graph feature expansion.

## Now

- Implement and validate epoch 3 `meta + graph_data + adj_out + adj_in` storage
  layout.
- Keep public Rust API and Mini-Cypher behavior unchanged.
- Reject epoch 2 database directories with `StorageFormatMismatch`; no migration
  is planned before 0.1.
- Prepare and publish 0.0.7 after release validation.
- Document traversal regression honestly; do not present 0.0.7 as a universal
  performance release.

## Next

- After 0.0.7, stop proactive database work and use NervusDB in downstream
  Agent Memory / local graph projects. Open new database work only from concrete
  downstream blockers.

## Later

- Benchmark regression detection for the core path.
- Release mechanics and publish documentation.
- Community contribution guide.

## Milestones

| Milestone | Target | Evidence |
|---|---|---|
| Contract reset | Q2 2026 | ADR 0005, active plan 010, docs updated |
| Boundary clean | Q2 2026 | Query has no storage dependency |
| Storage boring | Q2 2026 | Fjall reopen tests, crash recovery script passes |
| Query boring | Q2 2026 | Core tests match Mini-Cypher reference |
| API obvious | Q2 2026 | Directory path API documented and tested |
| 0.1 credible | Q2 2026 | Examples runnable, recovery proven, 10k/50k smoke passes |
| 0.0.1 release | Q2 2026 | Single `nervusdb` crate published to crates.io, docs complete, validation repeatable |
| 0.0.2 write path | Q2 2026 | 100k/500k benchmark stage timing and at least 2x insert improvement |
| 0.0.3 graph integrity | Q2 2026 | Dangling-edge rejection, tombstone cleanup tests, and release dry-run pass |
| 0.0.4 property equality index | Q2 2026 | Implemented locally: 100k-node scan 68,519.803 ms, index 1.435 ms, 47,757.312x speedup |
| 0.0.5 stability freeze | Q2 2026 | Released: fsck-lite, derived index repair, Agent Memory smoke, workspace tests passed |
| 0.0.6 performance hot path | Q2 2026 | Released: load total 1,674.287 ms, update p99 5,010.542 us, detach delete p99 6,480.459 us, two-hop 3,356,928 paths/s on 100k/500k medium benchmark |
| 0.0.7 storage cleanup | Q2 2026 | Release prep: epoch 3, clean reopen 3.185 ms, footprint 38.3 MB, traversal regression documented |

## Open Questions

- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
