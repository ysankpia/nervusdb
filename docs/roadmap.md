# Roadmap

## Current Phase

NervusDB 0.0.5 is in release preparation as the stability-freeze line for the
single public `nervusdb` crate.

## Now

- Publish 0.0.5 after release dry-run and remote CI pass.
- Keep repair conservative: rebuild `label_nodes` and `idx_node_props`; report
  canonical-data problems without deleting user graph data.
- Treat Agent Memory smoke as the stop-line proof for downstream use.

## Next

- Stop proactive database work after 0.0.5 unless a real downstream blocker
  appears.
- Use NervusDB in downstream projects and let those projects decide the next
  database task.

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
| 0.0.5 stability freeze | Q2 2026 | Implemented locally: fsck-lite, derived index repair, Agent Memory smoke, workspace tests passed |

## Open Questions

- Whether old bd PB tasks should be closed as superseded once ADR 0005 is fully
  implemented.
