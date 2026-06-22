# Roadmap

## Current Phase

NervusDB 0.0.6 performance work is in progress after cross-database benchmarks
showed concrete storage hot-path gaps against SQLite graph schemas.

## Now

- Keep 0.0.5 usable for downstream projects.
- Fix benchmark attribution before drawing storage architecture conclusions.
- Optimize proven storage hot paths without weakening `SyncAll` durability or
  expanding Mini-Cypher.

## Next

- Use 0.0.6 benchmark/profile evidence to decide whether keyspace merge needs a
  separate ADR.

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
| 0.0.6 performance hot path | Q2 2026 | In progress: cross-db medium benchmark exposes commit/reopen/mutation/traversal gaps |

## Open Questions

- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
