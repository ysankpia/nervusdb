# Dependency Policy

## Workspace Structure

The workspace defined in `Cargo.toml` has 5 members:

```text
nervusdb         — public Rust crate and real implementation owner
nervusdb-api     — local publish=false wrapper for nervusdb::api
nervusdb-storage — local publish=false wrapper for nervusdb::storage
nervusdb-query   — local publish=false wrapper for nervusdb::query
nervusdb-cli     — local debug/import/query/write CLI
```

ADR 0006 makes `nervusdb` the only public 0.0.1 crates.io target. The wrapper
crates exist only to keep local tests and scripts cheap during consolidation;
they must not become independent compatibility contracts.

## Internal Dependency Rules

1. **Core crates must not depend on frozen platform work.** Python, Node.js, C
   bindings, vector/HNSW, and full TCK code are not workspace core before 0.1.

2. **`nervusdb::api` is the query/storage boundary.** `nervusdb::storage` and
   `nervusdb::query` share IDs, `PropertyValue`, `GraphSnapshot`, and
   write-boundary traits through that module. Query and storage do not depend on
   each other directly.

3. **`nervusdb` owns the implementation.** `api`, `storage`, `query`, and the
   facade are modules in the public crate. Wrapper crates re-export these
   modules and point inward to `nervusdb`, not the other way around.

4. **`nervusdb-cli` depends on `nervusdb`.** It must not reach into
   wrapper crates or duplicate storage/query implementation.

5. **Query cannot import storage types.** If query needs a type or trait, move it
   to `nervusdb::api`.

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
nervusdb-cli -> nervusdb
nervusdb-api -> nervusdb
nervusdb-storage -> nervusdb
nervusdb-query -> nervusdb
```

Any direct dependency from `nervusdb::query` implementation code to
`nervusdb::storage` implementation code is a boundary violation. Use
`nervusdb::api`.

## Public Package Rule

For 0.0.1, users should depend on one crate:

```toml
[dependencies]
nervusdb = "0.0.1"
```

Do not publish `nervusdb-api`, `nervusdb-storage`, or `nervusdb-query` as
independent crates for 0.0.1 just to satisfy Cargo packaging. If the current
workspace shape blocks publishing `nervusdb` alone, refactor the package shape
or merge internal crates into modules before release.
