# API Surface

The 0.1 API is Rust-first. Public surface should make embedded local graph use
obvious and should not be shaped by bindings before the Rust core is credible.

## Core API

- `Db::open`
- `Db::open_paths`
- `Db::snapshot`
- `Db::begin_write`
- node, edge, label, relationship type, and property write paths
- read snapshot traversal paths
- query prepare/execute for Mini-Cypher

## Experimental Or Maintenance API

- `create_index`
- `search_vector`
- `backup`
- `vacuum`
- `compact`
- `checkpoint`
- binding-facing compatibility wrappers

Do not remove these in the first harness normalization pass. Classify first,
then decide later whether to feature-gate, hide from docs, or move modules.

