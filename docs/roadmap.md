# Roadmap

## Current Phase

NervusDB 0.0.3 has been released as the single public `nervusdb` crate. The
0.0.4 node property equality indexing is implemented and validated locally.
Release preparation is the next step.

## Now

- Prepare 0.0.4 release metadata and version bump.
- Keep the public crate shape as single-crate `nervusdb`.
- Keep the index exact-match only and internal; do not add public index
  management APIs during release prep.

## Next

- Publish 0.0.4 only after dry-run and CI confirmation.
- Start 0.0.5 planning around index audit/rebuild or benchmark regression
  detection.

## Later

- Index consistency audit / fsck-lite after 0.0.4.
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

## Open Questions

- Whether 0.0.5 should prioritize index audit/rebuild tooling or broader query
  ergonomics after property equality lookup lands.
- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
