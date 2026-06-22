# 016 Stability Freeze 0.0.5

## Status

Implemented locally; release preparation in progress.

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

- [x] Clean database fsck returns ok.
- [x] Fsck detects stale and missing `label_nodes` entries.
- [x] Fsck detects stale and missing `idx_node_props` entries.
- [x] Repair rebuilds `label_nodes` and `idx_node_props`.
- [x] Repair reports but does not auto-repair adjacency mismatch, orphan node props,
  or orphan edge props.
- [x] CLI JSON output has stable top-level fields: `ok`, `repaired`, `checked`,
  `issues`, `repairs`.
- [x] Agent Memory smoke passes, including reopen and fsck.
- [x] `cargo test --workspace` passes before release.

## Implementation Evidence

- `532b04b5 feat(admin): add fsck-lite core`
- `c701327f feat(cli): expose v2 fsck command`
- `245b11cc test(core): add fsck and agent memory smoke`

Fsck is feature-gated through `unstable-admin` and remains outside the 0.1
stable Rust API. The CLI enables that feature internally.

Repair remains conservative: it rebuilds only `label_nodes` and
`idx_node_props` from canonical keyspaces. It reports adjacency mismatches,
orphan node properties, and orphan edge properties without deleting user graph
data.

## Validation Evidence

Passed locally on 2026-06-22:

```bash
cargo fmt --all -- --check
cargo check -p nervusdb --examples
cargo test -p nervusdb --lib --features unstable-admin admin::tests
cargo clippy -p nervusdb --lib --features unstable-admin -- -D warnings
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb-cli
cargo test -p nervusdb --test core_0_1_agent_memory
cargo test -p nervusdb --features unstable-admin --test core_0_1_agent_memory
bash scripts/core_smoke.sh
bash scripts/check.sh
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
bash scripts/core_bench.sh --small
cargo test --workspace
```

Small benchmark artifact:

```text
artifacts/core-bench/core-bench-small-20260622-081528.json
```

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
