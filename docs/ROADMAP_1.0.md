# NervusDB 1.0 Roadmap (Revised: Post-MVP Pivot)

> **Goal**: Transform from "Skeleton MVP" to "Usage Ready".
> **Strategy**: Defer v1.0 release. Focus on core usability (String Labels) and foundation (Indexing) to bridge the gap between "toy" and "tool".

## Phase 1: Usability (v0.2.0 - The "Human" Update)
**Goal**: Users write Cypher with names, not internal IDs.

- [ ] **Must-Have**: String Label & Type Support in Cypher
    - `MATCH (n:Person)` instead of `MATCH (n)-[:1]->(m)`.
    - Auto-interning integration in Parser/Planner.
- [ ] **Must-Have**: Resilience Testing (Chaos Engineering)
    - "Kill-Verify-Restart" loop. Ensure `LabelInterner` metadata is perfectly durable.
- [ ] **Nice-to-Have**: Error Codes
    - Replace generic strings with `Error::Syntax`, `Error::Storage` etc.

## Phase 2: Performance (v0.3.0 - The "Index" Update)
**Goal**: Solve the "Full Scan" bottleneck.

- [ ] **Must-Have**: Secondary Indexes
    - `CREATE INDEX ON :Person(name)`.
    - In-memory B-Tree (MVP) or Disk-based B-Tree (Ideal).
    - Query Optimizer support for `Plan::IndexScan`.
- [ ] **Must-Have**: Disk-based IdMap
    - Move `HashMap<ExternalId, InternalId>` to `redb` or on-disk hash table to cap memory usage.

## Phase 3: Advanced (v1.0.0 - Production Ready)
**Goal**: Optimization and Concurrency.

- [ ] **Optimizer**: Join reordering based on statistics.
- [ ] **Advanced Traversal**: Bi-directional search / BFS for variable length paths.
- [ ] **Concurrency**: MVCC for non-blocking reads during high writes.

## Immediate Action Plan (Sprint 1)

1.  **T65**: Query Engine Upgrade - Support String Labels/Types.
2.  **T66**: Persistence Verification - "Kill -9" tests for metadata.
3.  **T67**: API Cleanup - Hide `InternalId`, finalize public facade.
