# Roadmap

## Current Phase

NervusDB 0.0.3 is being prepared as the next single public `nervusdb` crate
release. The line is graph integrity, not feature expansion.

## Now

- Prepare the v0.0.3 release after graph integrity validation passed.
- Keep v0.0.3 scoped to dangling-edge rejection, invalid mutation rejection,
  and detach-clean direct Rust API node deletion.
- Preserve 0.0.2 write-path performance and default `SyncAll` durability.

## Next

- Publish v0.0.3 after release dry-run and GitHub Actions pass.
- Start the next correctness plan only after v0.0.3 is published.

## Later

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
| 0.0.3 graph integrity | Q2 2026 | Dangling-edge rejection, tombstone cleanup tests, and release dry-run pass |

## Open Questions

- Whether property equality indexes deserve a post-write-path `prop_index` ADR.
- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
