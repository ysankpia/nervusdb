# Dependency Policy

## Workspace Structure

The workspace (defined in `Cargo.toml`) has 7 members:

```
nervusdb         — public Rust facade
nervusdb-api     — graph traits and shared IDs (query/storage boundary)
nervusdb-storage — page store, WAL, recovery, labels, properties, indexes
nervusdb-query   — Mini-Cypher parser/planner/executor
nervusdb-cli     — local debug/import/query/write CLI
nervusdb-pyo3    — Python bindings (experimental)
nervusdb-capi    — C API bindings (experimental)
```

## Internal Dependency Rules

1. **Core crates must not depend on experimental crates.**
   `nervusdb`, `nervusdb-api`, `nervusdb-storage`, and `nervusdb-query` must not
   depend on `nervusdb-pyo3`, `nervusdb-capi`, or `nervusdb-node`.

2. **`nervusdb-api` is the query/storage boundary.** `nervusdb-storage` and
   `nervusdb-query` depend on `nervusdb-api` for `GraphSnapshot` and related
   traits. They do not depend on each other.

3. **`nervusdb` (facade) depends on `nervusdb-storage`, `nervusdb-query`, and
   `nervusdb-api`.** It is the only crate that composes storage and query.

4. **`nervusdb-cli` depends on `nervusdb`** (the facade). It must not reach into
   `nervusdb-storage` or `nervusdb-query` directly.

5. **Experimental bindings depend on `nervusdb-capi`, not on `nervusdb`
   directly.** This is an accepted exception before 0.1.

## External Dependency Rules

6. **Do not add dependencies for simple local logic.** Prefer standard library or
   minimal code over pulling in a crate for a small utility.

7. **Justify every new external dependency before adding it.** The justification
   must be documented in the commit message or PR body.

8. **Pin dependencies to versions in `Cargo.lock`.** Do not use wildcard
   (`*`) or bare major-version requirements without reason.

9. **Prefer crates that are well-maintained, widely used, and compatible with
   the project's license (AGPL-3.0).**

## Experimental / Frozen Code

10. **Experimental and frozen code can use additional dependencies that core
    code does not.** However, those dependencies must not leak into core crate
    build artifacts or increase core crate compile time.

11. **Before 0.1, experimental bindings (nervusdb-pyo3, nervusdb-capi,
    nervusdb-node) remain workspace members.** Moving them out of the workspace
    is a post-0.1 consideration.

## Dependency Change Workflow

1. Add the dependency to the relevant `Cargo.toml`.
2. Update this policy document if the dependency affects validation, build, or
   security boundaries.
3. Run `bash scripts/check.sh` to verify the build and core tests still pass.
4. Run the full workspace test (`bash scripts/workspace_full_test.sh`) only if
   the dependency change crosses broad workspace boundaries.

## Current State

As of 2026-06-14, the workspace dependency graph (core only) is:

```
nervusdb-cli -> nervusdb -> nervusdb-storage
                          -> nervusdb-query -> nervusdb-api
                          -> nervusdb-api

nervusdb-storage -> nervusdb-api
```

Experimental crates (`nervusdb-pyo3`, `nervusdb-capi`) depend on `nervusdb`
directly or through wrapper layers. They are not part of the default validation
loop.
