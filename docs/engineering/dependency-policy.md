# Dependency Policy

## Workspace Structure

The workspace defined in `Cargo.toml` has 5 members:

```text
nervusdb         — public Rust facade
nervusdb-api     — graph traits, shared IDs, PropertyValue, query/storage boundary
nervusdb-storage — Fjall-backed local graph storage
nervusdb-query   — Mini-Cypher parser/planner/executor
nervusdb-cli     — local debug/import/query/write CLI
```

## Internal Dependency Rules

1. **Core crates must not depend on frozen platform work.** Python, Node.js, C
   bindings, vector/HNSW, and full TCK code are not workspace core before 0.1.

2. **`nervusdb-api` is the query/storage boundary.** `nervusdb-storage` and
   `nervusdb-query` depend on `nervusdb-api` for shared IDs, `PropertyValue`,
   `GraphSnapshot`, and write-boundary traits. They do not depend on each other.

3. **`nervusdb` composes storage and query.** The facade depends on
   `nervusdb-storage`, `nervusdb-query`, and `nervusdb-api`.

4. **`nervusdb-cli` depends on `nervusdb`.** It must not reach into
   `nervusdb-storage` directly.

5. **Query cannot import storage types.** If query needs a type or trait, move it
   to `nervusdb-api`.

## External Dependency Rules

6. **Do not add dependencies for simple local logic.** Prefer standard library or
   minimal code over pulling in a crate for a small utility.

7. **Justify every new external dependency before adding it.** The justification
   must be documented in the commit message, PR body, or relevant ADR.

8. **Pin dependencies through `Cargo.lock`.** Do not use wildcard (`*`) or bare
   major-version requirements without reason.

9. **Prefer crates that are well-maintained, widely used, pure Rust when
   possible, and compatible with the project's AGPL-3.0 licensing.**

## Approved 0.1 Storage Dependency

Fjall is approved by ADR 0005 as the 0.1 local KV/LSM storage substrate. This is
an exception to the normal "avoid new dependencies" bias because it removes a
larger and riskier self-built storage-engine surface: Pager, WAL, B+Tree, CSR,
and read-path merge logic.

## Dependency Change Workflow

1. Add the dependency to the relevant `Cargo.toml`.
2. Update this policy document if the dependency affects validation, build, or
   security boundaries.
3. Run focused tests for the touched crate.
4. Run `bash scripts/check.sh` for broad refactors.
5. Run the full workspace test only when the dependency change crosses broad
   workspace boundaries.

## Current Intended Graph

```text
nervusdb-cli -> nervusdb -> nervusdb-storage -> nervusdb-api
                          -> nervusdb-query   -> nervusdb-api
                          -> nervusdb-api
```

Any direct `nervusdb-query -> nervusdb-storage` dependency is a boundary
violation to remove before the Fjall backend lands.
