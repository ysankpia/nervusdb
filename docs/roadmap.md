# Roadmap

## Current Phase

Fjall storage refactor landed in the current working tree; 0.1 is now in
validation and cleanup mode.

The previous slimming work made the repository smaller. The Fjall refactor
removed the self-built Pager/WAL/B+Tree/CSR direction and replaced it with
Fjall-backed logical graph keyspaces.

## Now

- Prepare 0.0.1 as a single public `nervusdb` crate release.
- Keep the post-Fjall API surface small: `checkpoint` and `close` stay as
  lifecycle helpers; old compaction and property-index hooks stay out.
- Run release-scale manual smoke when the API surface is otherwise stable.

## Next

- Push main and wait for CI.
- Refactor or package the workspace so `nervusdb` is the only public crate needed
  on crates.io.
- Run release-readiness validation, medium benchmark, and publish dry-run.
- Tag and publish `v0.0.1`.

## Later

- 0.0.2 correctness work: dangling-edge rejection and tombstone cleanup.
- Property equality index ADR if real usage or benchmarks justify it.
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

## Open Questions

- Whether property equality indexes deserve a post-0.1 `prop_index` ADR.
- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
