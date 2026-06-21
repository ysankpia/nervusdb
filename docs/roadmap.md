# Roadmap

## Current Phase

Fjall storage refactor landed in the current working tree; 0.1 is now in
validation and cleanup mode.

The previous slimming work made the repository smaller. The Fjall refactor
removed the self-built Pager/WAL/B+Tree/CSR direction and replaced it with
Fjall-backed logical graph keyspaces.

## Now

- Review and commit the Fjall refactor.
- Clean query warning/MSRV clippy debt separately.
- Decide whether `Db::compact/checkpoint/close` remain explicit maintenance
  wrappers or are simplified after 0.1.
- Run release-scale manual smoke when the API surface is otherwise stable.

## Next

- Large manual acceptance smoke after Fjall storage stabilizes.
- Benchmark baseline for the core path.
- Property-index ADR if equality/range indexes become worth promoting.
- Release-readiness pass over docs, examples, and crates.io metadata.

## Later

- Property index ADR if equality/range indexes become core.
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
| 0.1 release | Q2 2026 | Published to crates.io, docs complete, validation repeatable |

## Open Questions

- Whether `Db::compact/checkpoint/close` remain compatibility methods or become
  no-op/maintenance wrappers under Fjall.
- Whether property equality indexes deserve a post-0.1 `prop_index` ADR.
- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
