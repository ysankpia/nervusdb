# Codebase Analysis

This is the current source map after the Fjall storage refactor and query scope
pruning on 2026-06-21. Older pre-Fjall CodeGraph notes are historical evidence
only and must not be used to infer current architecture.

## Workspace

`Cargo.toml` currently has five workspace members:

```text
nervusdb-cli
nervusdb
nervusdb-api
nervusdb-query
nervusdb-storage
```

Experimental binding directories may remain in the repository, but they are not
workspace members and do not define the 0.1 embedded graph core.

## Core Crates

```text
nervusdb          public Rust facade: Db, snapshots, write transactions
nervusdb-api      storage/query boundary traits and shared value types
nervusdb-storage  Fjall-backed graph keyspaces and write transaction engine
nervusdb-query    Mini-Cypher parser/planner/executor for 0.1
nervusdb-cli      local query/write/repl smoke tool
```

The intended dependency shape is:

```text
nervusdb-cli -> nervusdb
nervusdb     -> nervusdb-api + nervusdb-storage + nervusdb-query
nervusdb-storage -> nervusdb-api
nervusdb-query   -> nervusdb-api
```

`nervusdb-query` must not depend on `nervusdb-storage`. Storage implements the
API traits; query consumes the traits.

## Storage Shape

The current storage model is a local database directory managed by Fjall.
NervusDB exposes a logical graph storage contract, not Fjall's internal files.

Logical keyspaces:

```text
meta
nodes
ext2node
labels
reltypes
node_labels
label_nodes
adj_out
adj_in
node_props
edge_props
```

Current storage rules:

- `Db::open(path)` opens a directory.
- old file-pair storage compatibility is not a 0.1 goal.
- edge identity is `(src_iid, rel_type_id, dst_iid)`.
- parallel edges are not supported before 0.1.
- label IDs and relationship type IDs are separate namespaces.
- property keys are original strings, not hashes.
- property indexes are not 0.1 core and have no public API hook before a future
  ADR defines them.

## Query Shape

The current main query path is Mini-Cypher 0.1. Unsupported openCypher breadth
must fail fast with an `outside Mini-Cypher 0.1` error instead of compiling into
an executable plan.

Supported core surface:

- constant `RETURN`
- node scan
- label scan
- directed one-hop traversal
- documented two-hop traversal
- simple equality filters
- simple projection
- `LIMIT`
- basic `CREATE`
- basic `SET n.key = value`
- basic `DELETE`
- `EXPLAIN`

Frozen before 0.1:

- optional match
- with/union/unwind
- merge/foreach/call/remove
- return distinct
- ordering and skip
- broad aggregation
- named paths and variable-length paths
- subqueries, procedures, list comprehension, and pattern comprehension
- full compatibility test pass rate as a success metric

## Current Risk Register

| Risk | Status | Action |
|---|---|---|
| Public docs drift after storage replacement | Active cleanup | Keep README, rustdoc, CLI help, and docs aligned with directory storage |
| Property index hooks | Retired debt | Removed from the public API until a future ADR defines `prop_index` |
| Lifecycle persistence helpers | Accepted API | `checkpoint` and `close` are explicit helpers over Fjall persistence |
| Historical docs in current paths | Active cleanup | Mark as historical or replace with current source maps |
| Large-scale durability evidence | Manual gate | Run documented large and crash/reopen checks for release candidates |

## Validation Map

Default local validation:

```bash
bash scripts/check.sh
```

Focused 0.1 validation:

```bash
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_mini_cypher
cargo test -p nervusdb --test core_0_1_examples
bash scripts/core_examples.sh
bash scripts/core_crash_recovery.sh
```

Manual full validation:

```bash
bash scripts/workspace_full_test.sh
```
