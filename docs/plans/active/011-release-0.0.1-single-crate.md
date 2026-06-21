# 011 Release 0.0.1 As A Single Public Crate

## Status

In progress. The single-crate package shape has been implemented and locally
validated. Push, CI, tag, and crates.io publish are still pending.

## Goal

Prepare and publish NervusDB 0.0.1 as one user-facing crate, `nervusdb`, while
preserving the clean internal architecture created by the Fjall storage refactor
and query pruning work.

The release target is:

```toml
[dependencies]
nervusdb = "0.0.1"
```

not a family of public crates.

## Scope

- Convert or package the current workspace so crates.io users only see
  `nervusdb`.
- Keep the current internal boundaries conceptually: API traits/types, storage,
  query, facade, and CLI/debug workflows.
- Keep Mini-Cypher 0.1 scope unchanged.
- Keep Fjall-backed local directory storage unchanged.
- Run release validation and publish dry-run before tagging.

## Not In Scope

- 0.0.2 feature work.
- Property indexes.
- delete GC changes.
- dangling-edge enforcement.
- Edge IDs or parallel edges.
- HNSW/vector/GraphRAG.
- Python, Node, or C bindings.
- Multi-writer OCC.
- Separate CLI crate publication.

## Release Gate

0.0.1 is ready to tag only when:

- `git status --short --branch` is clean.
- `git push origin main` has landed the current commits.
- GitHub Actions is green for `main`.
- public package shape is single crate `nervusdb`.
- `bash scripts/check.sh` passes.
- `bash scripts/core_examples.sh` passes.
- `bash scripts/core_crash_recovery.sh` passes.
- `cargo test --workspace` passes.
- medium benchmark evidence is recorded or explicitly deferred with reason.
- `cargo publish -p nervusdb --dry-run` passes.
- README and docs describe only `nervusdb` as the public dependency.

## Steps

1. Record ADR 0006 single-crate public release decision.
2. Choose the implementation path for package shape.
3. Refactor package shape so `nervusdb` is the only public crate required for
   `cargo publish`.
4. Update README, README_CN, architecture docs, dependency policy, release
   readiness, and progress.
5. Run validation:

   ```bash
   cargo fmt --all -- --check
   bash scripts/check.sh
   cargo test --workspace
   bash scripts/core_examples.sh
   bash scripts/core_crash_recovery.sh
   ```

6. Run medium benchmark:

   ```bash
   bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000
   ```

7. Run publish dry-run:

   ```bash
   cargo publish -p nervusdb --dry-run
   ```

8. Push main and wait for CI.
9. Tag `v0.0.1`.
10. Publish `nervusdb` 0.0.1.

## Validation

Record command output and benchmark artifact path in `PROGRESS.md` before
tagging.

Current local evidence:

- `nervusdb` owns the real implementation under `nervusdb/src/{api.rs,query,storage}`.
- `nervusdb-api`, `nervusdb-storage`, and `nervusdb-query` are
  `publish = false` local wrapper crates.
- `scripts/core_bench.sh` runs the benchmark through the public `nervusdb`
  crate, not the storage wrapper.
- `cargo publish -p nervusdb --dry-run --registry crates-io --allow-dirty`
  passed before commit. A clean dry-run is still required after commit.
- Commit `0cd081fc` created the package-shape refactor.
- `cargo publish -p nervusdb --dry-run --registry crates-io` passed after
  commit. The local `[patch.crates-io]` warnings are expected because the
  publish package no longer depends on the wrapper crates.
- The attempted medium benchmark
  `bash scripts/core_bench.sh --nodes 100000 --degree 5 --iters 1000` was
  stopped after it did not finish within the interactive release window. It is
  deferred for post-release tuning and is not used as 0.0.1 evidence.

## Completion Evidence

- release commit hash
- CI link or status
- benchmark artifact path
- dry-run result
- tag hash
- crates.io release URL

## Remaining Risks

- Packaging refactor can disturb module imports even if behavior stays the
  same. Use tests, not visual inspection, as the acceptance signal.
- Cargo publish may expose metadata gaps that local tests do not cover.
- Medium benchmark is evidence, not a proof of production-scale performance.
