# Plan 006: CLI Examples Validation

## Status

Completed

## Goal

Provide concrete 0.1 usage evidence: CLI smoke workflows, ten realistic graph
examples, crash recovery proof, and small reproducible benchmark commands.

## Scope

- Keep CLI docs limited to local smoke/debug/import/query/write.
- Document ten realistic 0.1 user stories and map them to examples or commands.
- Maintain `scripts/core_smoke.sh`, `scripts/core_crash_recovery.sh`, and
  `scripts/core_bench.sh`.
- Keep large 1M node / 5M edge evidence manual and recorded.

## Not In Scope

- Turning the CLI into a platform product.
- Broad ETL tooling.
- Release-window automation.
- Perf gates as a default development tax.

## Steps

1. Audit CLI commands used by README and reference docs.
2. Align examples with `docs/product/user-stories-0.1.md`.
3. Keep smoke, crash, and benchmark scripts small by default.
4. Add documentation for large manual acceptance runs.
5. Record example validation evidence before 0.1 readiness.

## Validation

- `bash scripts/core_examples.sh` for ten runnable 0.1 CLI examples.
- `bash scripts/core_smoke.sh`.
- `bash scripts/core_crash_recovery.sh` for recovery evidence.
- `bash scripts/core_bench.sh --small` for local benchmark sanity.
- `bash scripts/check.sh` before commit.

## Docs To Update

- `docs/reference/cli.md`
- `docs/product/user-stories-0.1.md`
- `docs/runbooks/local-validation.md`
- `docs/runbooks/benchmark-validation.md`
- `docs/runbooks/release-readiness.md`

## Completion Evidence

Completed on 2026-05-17.

CLI contract:

- `docs/reference/cli.md` documents only the real 0.1 CLI command surface:
  `v2 query`, `v2 write`, `v2 repl`, and maintenance-only `v2 vacuum`.
- Import-style workflow is defined as file-driven smoke using
  `v2 write --file`; no stable import subcommand was added.

Runnable examples:

- Added `docs/reference/examples-0.1.md`.
- Added ten example fixture directories under `examples/core-0.1/`.
- Added `bash scripts/core_examples.sh`.
- `bash scripts/core_examples.sh` passed:
  - `01-social`
  - `02-dependency`
  - `03-file-module`
  - `04-knowledge`
  - `05-hierarchy`
  - `06-tags`
  - `07-ownership`
  - `08-crates`
  - `09-recommendation`
  - `10-import-then-query`

Validation run:

- `bash -n scripts/core_examples.sh scripts/core_smoke.sh scripts/core_crash_recovery.sh scripts/core_bench.sh` passed.
- `bash scripts/core_examples.sh` passed: ten examples, fresh temp DB per example,
  expected NDJSON matched.
- `bash scripts/core_smoke.sh` passed: wrote `Person -KNOWS-> Person` and read
  `{"b.name":"Bob"}`.
- `bash scripts/core_crash_recovery.sh` passed with defaults:
  `iterations=5`, `batch=64`, `node_pool=64`, `rel_pool=8`.
- `bash scripts/core_bench.sh --small` passed with:
  `nodes=1000`, `degree=5`, `edges=5000`, `iters=100`, `write_iters=20`.

Manual-only evidence:

- Large benchmark remains manual release-candidate evidence:
  `bash scripts/core_bench.sh --large`.
- `bash scripts/workspace_full_test.sh` was intentionally skipped. Phase 006
  touched CLI docs, core examples, focused validation docs, and plan evidence;
  it did not change broad workspace behavior, bindings, full TCK, or platform
  release gates.
