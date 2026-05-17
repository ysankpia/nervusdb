# Validation Policy

Validation cost must match blast radius. The default path is intentionally fast;
full historical fan-out is manual.

| Change | Required validation |
|---|---|
| Docs only | Link/path grep for touched docs. Run `bash -n` only for touched scripts. No Rust test by default. |
| CI or shell scripts | `bash -n` for touched scripts plus a targeted dry run or grep proving the trigger shape. |
| Mini-Cypher | `bash scripts/workspace_quick_test.sh` plus a targeted query test when behavior changed. |
| Rust API facade | Focused facade test, example, or doctest proving the public path. |
| Storage or WAL | Targeted storage test plus `bash scripts/core_crash_recovery.sh` when recovery can be affected. |
| CLI | `bash scripts/core_smoke.sh` or a targeted CLI command against a temp database. |
| Broad refactor | `bash scripts/check.sh`; run `bash scripts/workspace_full_test.sh` only when the touched surface justifies it. |
| Release readiness | Core check, core smoke, crash recovery evidence, small benchmark, and documented manual checks. |

## Default Commands

Normal development:

```bash
bash scripts/check.sh
```

Quick query acceptance:

```bash
bash scripts/workspace_quick_test.sh
```

Manual full verification:

```bash
bash scripts/workspace_full_test.sh
```

Do not hide `workspace_full_test.sh` behind `check`, `quick`, `pre-commit`, or
`pre-push`.
