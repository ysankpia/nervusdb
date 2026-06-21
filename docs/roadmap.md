# Roadmap

## Current Phase

NervusDB 0.0.1 has been released as the single public `nervusdb` crate. The
next line is 0.0.2 write-path and bulk-import work.

## Now

- Make benchmark output stage-aware.
- Improve bulk write staging without changing public API or `SyncAll`
  durability.
- Use the 0.0.1 100k/500k benchmark as the baseline.

## Next

- Reach at least 2x faster 100k/500k insert throughput before 0.0.2 release.
- Record stage timing and artifact paths in the active 0.0.2 plan and
  `PROGRESS.md`.

## Later

- Correctness work: dangling-edge rejection and tombstone cleanup.
- Property equality index ADR after write-path cost is understood.
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

## Open Questions

- Whether property equality indexes deserve a post-write-path `prop_index` ADR.
- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
