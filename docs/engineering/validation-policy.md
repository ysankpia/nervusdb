# Validation Policy

Validation cost must match blast radius. The default path is intentionally fast;
full historical fan-out is manual.

| Change | Required validation |
|---|---|
| Docs only | Link/path grep for touched docs. Run `bash -n` only for touched scripts. No Rust test by default. |
| CI or shell scripts | `bash -n` for touched scripts plus a targeted dry run or grep proving the trigger shape. |
| Mini-Cypher | `bash scripts/workspace_quick_test.sh` plus a targeted query test when behavior changed. |
| Rust API facade | Focused facade test, example, or doctest proving the public path. |
| Query/storage boundary | `cargo test -p nervusdb --test core_0_1_mini_cypher`, local wrapper smoke checks, and a grep proving `nervusdb/src/query` does not import `crate::storage`. |
| Fjall storage backend | Targeted storage tests, reopen tests, and `bash scripts/core_crash_recovery.sh` when recovery can be affected. |
| Storage format contract | Storage model/reference docs plus focused reopen/crash validation. |
| CLI | `bash scripts/core_smoke.sh` or a targeted CLI command against a temp database directory. |
| Broad refactor | `bash scripts/check.sh`; run the full test suite manually when the touched surface justifies it. |
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

Fjall storage refactor focused path:

```bash
cargo check -p nervusdb-api -p nervusdb-query -p nervusdb-storage --lib
rg -n "crate::storage|nervusdb::storage" nervusdb/src/query
cargo test -p nervusdb-storage --test core_0_1_storage
cargo test -p nervusdb --test core_0_1_rust_api
cargo test -p nervusdb --test core_0_1_mini_cypher
bash scripts/core_crash_recovery.sh
bash scripts/check.sh
```
