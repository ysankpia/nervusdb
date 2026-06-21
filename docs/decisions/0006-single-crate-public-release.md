# ADR 0006: Single Crate Public Release

## Status

Accepted

## Context

NervusDB currently uses multiple Rust crates as engineering boundaries:

- `nervusdb`
- `nervusdb-api`
- `nervusdb-storage`
- `nervusdb-query`
- `nervusdb-cli`

That structure is useful for development because it keeps query, storage, API
traits, facade, and CLI responsibilities visible. It is not automatically the
right product packaging.

For crates.io, publishing `nervusdb` as it stands would also require publishing
`nervusdb-api`, `nervusdb-storage`, and `nervusdb-query` first, because registry
crates cannot depend on unpublished path-only crates. Users would install only
`nervusdb`, but the public package surface would still expose several NervusDB
internal crates.

For a pre-1.0 embedded database, that is the wrong default. The product is one
database, not a family of independently supported libraries.

## Decision

The public 0.0.1 release target is a single user-facing crate:

```toml
[dependencies]
nervusdb = "0.0.1"
```

Internal boundaries may remain in the repository, but `nervusdb-api`,
`nervusdb-storage`, and `nervusdb-query` are not independent public release
products for 0.0.1.

Before publishing 0.0.1, the release implementation must choose one of these
mechanically valid approaches:

1. Merge internal crates into the `nervusdb` crate as internal modules.
2. Keep workspace crates for local development but make the published
   `nervusdb` package include them in a way Cargo can publish as one crate.

The preferred approach for 0.0.1 is option 1: merge into the `nervusdb` public
crate while preserving internal module boundaries such as `api`, `storage`, and
`query`.

The CLI is also not a separate public product before 0.0.1. CLI behavior can be
provided through the repository and examples first. A separate installable CLI
crate can be revisited after the library crate is useful and stable.

## Rationale

The simplest product contract wins:

- one project
- one public crate
- one README
- one docs.rs entry
- one release version
- one user import path

This is the packaging equivalent of the storage refactor: do not expose internal
engineering seams as public compatibility promises before the product needs
them.

The current internal separation still taught the codebase the right dependency
boundaries. That value is preserved by modules and tests. It does not require
publishing every internal layer as a public crate.

## Rejected Alternatives

### Publish every workspace crate

Rejected for 0.0.1. It creates public versioning and compatibility promises for
internal implementation layers. It also makes the release process noisier than
the product deserves.

### Publish only `nervusdb` while depending on unpublished internal crates

Rejected because crates.io cannot resolve unpublished path-only dependencies.
This is not a valid release mechanism.

### Keep internal crates public but tell users to ignore them

Rejected. Public packages are public contracts. Documentation cannot undo the
compatibility cost of publishing implementation crates.

## Consequences

- 0.0.1 release readiness must include a package-shape refactor or equivalent
  Cargo packaging solution before `cargo publish`.
- Release dry-run should target the public crate:

  ```bash
  cargo publish -p nervusdb --dry-run
  ```

- Internal crate-level checks can remain local validation until the merge is
  done, but they are not public release artifacts.
- Future separate crates are allowed only when they have a real external user:
  for example a stable CLI, a stable query engine crate, or a storage adapter
  ecosystem.

## Follow-Up Work

- Add a release plan for converting the workspace into a single public crate
  package.
- Update README, architecture, dependency policy, and release runbook to
  distinguish internal boundaries from public package boundaries.
- After the package-shape refactor, rerun:

  ```bash
  cargo fmt --all -- --check
  bash scripts/check.sh
  cargo test --workspace
  bash scripts/core_examples.sh
  bash scripts/core_crash_recovery.sh
  cargo publish -p nervusdb --dry-run
  ```
