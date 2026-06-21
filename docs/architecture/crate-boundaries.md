# Crate And Module Boundaries

## Public Package

`nervusdb` is the only public crate for the current line. Users should depend on:

```toml
[dependencies]
nervusdb = "0.0.2"
```

The query, storage, and API boundaries live inside that crate as modules:

```text
nervusdb::api      graph traits, shared IDs, PropertyValue, write boundary
nervusdb::storage  Fjall-backed graph keyspaces and transaction engine
nervusdb::query    Mini-Cypher parser/planner/executor for 0.1
```

`nervusdb-cli` remains a workspace-local smoke/debug/query/write tool. It
depends only on `nervusdb`.

## Local Wrapper Crates

`nervusdb-api`, `nervusdb-storage`, and `nervusdb-query` may remain in the
workspace as thin local wrappers while tests and scripts are being consolidated.
They re-export the implementation from `nervusdb`. They are not independent
current release products and must not be published to crates.io without a future ADR.

## Required Dependency Direction

```text
nervusdb-cli -> nervusdb
nervusdb-api -> nervusdb
nervusdb-storage -> nervusdb
nervusdb-query -> nervusdb
```

Inside the `nervusdb` crate, `query` must depend only on `api` traits/types, not
on `storage` implementation types. `storage` implements the `api` traits.
`nervusdb` facade composes both.

## Experimental Or Frozen Areas

- full openCypher
- full TCK harness
- vector/HNSW
- Python, Node.js, and C bindings
- cross-binding parity gates
- perf, chaos, soak, fuzz, benchmark, and stability matrices

Frozen means build/security maintenance is allowed. New capability work before
0.1 requires an ADR.
