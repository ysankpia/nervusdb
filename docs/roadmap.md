# Roadmap

## Current Phase

NervusDB 0.0.8 is in release preparation. The release closes the
0.0.7 traversal regression and clean-reopen issue without changing public API
or durability semantics.

## Now

- Validate ADR 0010 packed adjacency lists and close-time journal checkpoint.
- Keep public Rust API, Mini-Cypher behavior, and `SyncAll` durability unchanged.
- Treat the remaining durable commit cost as Fjall `SyncAll` batch persistence
  unless a future downstream workload proves a different owner.
- Keep 0.0.8 out of feature expansion.

## Next

- After 0.0.8, stop proactive database work unless real downstream projects
  expose a concrete blocker.

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
| 0.0.7 storage cleanup | Q2 2026 | Released: epoch 3, clean reopen 3.185 ms, footprint 38.3 MB, traversal regression documented |
| 0.0.8 performance closeout | Q2 2026 | Release prep: epoch 4 packed adjacency lists, raw reopen 2.059 ms, two-hop 4,905,668 paths/s, footprint 29.4 MB; remaining commit cost is Fjall `SyncAll` floor |

## Open Questions

- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
