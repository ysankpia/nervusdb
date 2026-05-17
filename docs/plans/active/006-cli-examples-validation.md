# Plan 006: CLI Examples Validation

## Status

Planned

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

Record commands, outputs, data scale, and any examples not yet runnable.
