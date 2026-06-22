# 018 Storage Layout 0.0.7

## Status

Planned. Implementation must not start until a storage-layout ADR is accepted.

## Goal

Attack the remaining large 0.0.6 performance costs: durable bulk commit, raw
database reopen, file count, and storage footprint.

The target is not more graph features. The target is a simpler physical layout
that behaves more like an embedded database users can carry around without
paying unnecessary multi-keyspace overhead.

## Evidence From 0.0.6

0.0.6 removed the easy hot-path bugs:

- old full `idx_node_props` cleanup scans are gone,
- adjacency scans stream directly,
- traversal no longer performs redundant per-edge node liveness reads,
- bulk property-index writes no longer linearly search `created_nodes` for every
  staged property.

Current medium evidence:

```text
command: bash scripts/cross_db_bench.sh --system nervusdb --medium
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-120945.ndjson
dataset: 100,000 nodes / 500,000 edges
```

| Metric | Current |
|---|---:|
| load total | 1,674.287 ms |
| durable commit | 1,570.171 ms |
| raw reopen | 3,249.434 ms |
| count verify after reopen | 105.118 ms |
| disk footprint | 84,595,889 bytes / 31 files |

Profile evidence:

```text
artifact: artifacts/cross-db-bench/cross-db-bench-medium-20260622-120913.ndjson
```

`WriteTxn::commit.property_index_writes` is down to `244.979 ms`; it is no
longer the main bottleneck. `batch.commit` and raw `GraphEngine::open.database`
are now the controlling costs.

## Candidate Direction

Evaluate collapsing the current many-keyspace physical layout into a smaller
number of Fjall keyspaces, likely one tagged graph data keyspace plus metadata:

```text
[tag][logical-key...]
```

Candidate tags:

```text
node record
external-id to node
label name
reltype name
node label
label node
out adjacency
in adjacency
node property
edge property
node property equality index
```

The exact tag bytes and key layout belong in the ADR, not this plan.

## Required ADR Questions

- Is this a destructive pre-0.1 format change or does it require migration?
- Does Fjall perform materially better with one or two keyspaces for this
  workload?
- Which key ranges must preserve prefix-scan locality?
- Can fsck-lite rebuild derived indexes after the layout change?
- What does crash recovery look like with the new layout?
- What benchmark evidence is enough to justify the format churn?
- Does the change reduce raw reopen, durable commit, and file count without
  regressing traversal and property lookup?

## Acceptance

Before implementation:

- ADR accepted.
- Current storage format documented as the old layout.
- New key layout documented with byte-level tags and prefix scan ranges.
- Migration or destructive-reset decision documented.

Implementation target:

- Public Rust API unchanged.
- Default durability remains `SyncAll`.
- Mini-Cypher behavior unchanged.
- Fsck-lite still works or is explicitly updated with tests.
- Medium benchmark correctness hash remains stable.
- Current property lookup and traversal wins do not regress materially.
- Raw reopen and durable commit improve enough to justify the storage format
  change.

## Non-Goals

- New graph features.
- Full Cypher.
- EdgeId or parallel edges.
- Vector/HNSW.
- Multi-writer concurrency.
- Unsafe durability modes.
- Public storage-format compatibility promises before 0.1.
