# Roadmap

## Current Phase

NervusDB 0.0.6 performance hot-path work is ready for release after
cross-database benchmarks exposed concrete gaps against SQLite graph schemas and
the easy storage hot paths were removed.

## Now

- Prepare and publish 0.0.6.
- Treat the 0.0.6 cross-database benchmark and storage profile as the current
  performance evidence.
- Record the remaining keyspace/open/commit costs as 0.0.7 storage-layout work.
- Do not start keyspace merge without a separate ADR.

## Next

- 0.0.7 should target storage layout only if the ADR proves it is worth the
  storage-format churn; otherwise stop database work and use NervusDB downstream.

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
| 0.0.6 performance hot path | Q2 2026 | Ready for release: load total 1,674.287 ms, update p99 5,010.542 us, detach delete p99 6,480.459 us, two-hop 3,356,928 paths/s on 100k/500k medium benchmark |
| 0.0.7 storage layout | Q2 2026 | Planned: ADR required before keyspace merge or storage-format rewrite |

## Open Questions

- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
