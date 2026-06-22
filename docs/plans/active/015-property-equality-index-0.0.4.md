# 015 Property Equality Index 0.0.4

## Status

Active.

## Goal

Make 0.0.4 a minimal node property equality index release. The goal is faster
query anchoring, not a broader query language or public index-management API.

## Scope

- Add internal `idx_node_props` keyspace for label + property key + encoded
  scalar value + node id.
- Add `GraphSnapshot::nodes_with_label_and_property` with a correct fallback.
- Make Fjall snapshots use `idx_node_props` for that method.
- Maintain index entries in the same commit batch as node property, label, and
  node deletion changes.
- Let Mini-Cypher anchor `MATCH (n:Label) WHERE n.key = scalar_literal` and
  `MATCH (n:Label {key: scalar_literal})` through the new snapshot method.
- Add benchmark evidence comparing scan baseline and indexed lookup.

## Not In Scope

- Public `create_index` / `lookup_index`.
- Range indexes, edge property indexes, composite indexes, or unique
  constraints.
- Parameter-based index planning.
- Full optimizer or cost model.
- Edge IDs, parallel edges, vectors, HNSW, multi-writer concurrency, or more
  Cypher syntax.

## Acceptance

- Storage tests prove index insert, update, removal, label add/remove,
  tombstone cleanup, same-transaction final-state behavior, and reopen
  consistency.
- Query tests prove label + scalar property equality still returns correct rows
  through `WHERE` and inline properties.
- Non-indexed query shapes still return correct rows through scan/filter.
- `bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000` records a
  property lookup speedup of at least 10x over the scan baseline.
- 100k/500k insert throughput must not fall below 50% of the best 0.0.2 recorded
  benchmark. If it does, 0.0.4 is blocked or must split index write-cost work.

## Required Validation

```bash
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
bash scripts/core_bench.sh --small
bash scripts/check.sh
cargo test -p nervusdb --test core_0_1_rust_api
cargo test -p nervusdb --test core_0_1_examples
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
cargo test --workspace
bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000
```

Release preparation still requires:

```bash
cargo publish -p nervusdb --dry-run --registry crates-io
```
