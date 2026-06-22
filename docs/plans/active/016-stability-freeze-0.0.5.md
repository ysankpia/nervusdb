# 016 Stability Freeze 0.0.5

## Status

Active.

## Goal

Make `v0.0.5` the final planned database-hardening release before NervusDB is
used as a dependency in downstream projects.

## Scope

- Add `nervusdb v2 fsck` CLI.
- Add `nervusdb::admin` behind feature `unstable-admin`.
- Check derived indexes and graph-storage invariants.
- Repair only rebuildable derived indexes: `label_nodes` and `idx_node_props`.
- Add Agent Memory smoke to prove the database supports a realistic downstream
  project pattern.
- Update docs to mark 0.0.5 as a stability freeze, not a feature expansion.

## Not In Scope

- Range indexes.
- Public index-management APIs.
- Edge IDs, parallel edges, vectors, HNSW, multi-writer work, or broader Cypher.
- Automatic deletion of canonical user graph data during repair.
- Long-term storage-format compatibility promises before 0.1.

## Acceptance

- Clean database fsck returns ok.
- Fsck detects stale and missing `label_nodes` entries.
- Fsck detects stale and missing `idx_node_props` entries.
- Repair rebuilds `label_nodes` and `idx_node_props`.
- Repair reports but does not auto-repair adjacency mismatch, orphan node props,
  or orphan edge props.
- CLI JSON output has stable top-level fields: `ok`, `repaired`, `checked`,
  `issues`, `repairs`.
- Agent Memory smoke passes, including reopen and fsck.
- `cargo test --workspace` passes before release.

## Required Validation

```bash
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb-cli
bash scripts/check.sh
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
bash scripts/core_bench.sh --small
cargo test --workspace
bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000
cargo publish -p nervusdb --dry-run --registry crates-io
```
