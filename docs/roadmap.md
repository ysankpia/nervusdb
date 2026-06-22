# Roadmap

## Current Phase

NervusDB 0.0.3 has been released as the single public `nervusdb` crate. The
next line is 0.0.4 node property equality indexing.

## Now

- Add an internally maintained node property equality index.
- Keep the index exact-match only and storage-neutral at the query boundary.
- Preserve 0.0.2 write-path performance and default `SyncAll` durability.

## Next

- Prove `MATCH (n:Label) WHERE n.key = literal` can anchor through the index.
- Record benchmark evidence for scan baseline versus indexed lookup.

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
| 0.0.4 property equality index | Q2 2026 | Exact property lookup is correct and at least 10x faster than scan baseline |

## Open Questions

- Whether 0.0.5 should prioritize index audit/rebuild tooling or broader query
  ergonomics after property equality lookup lands.
- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
