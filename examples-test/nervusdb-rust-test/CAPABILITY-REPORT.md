# NervusDB Rust Core Engine — Capability Test Report

> Updated: 2026-02-23
> Test entry: `examples-test/nervusdb-rust-test/tests/test_capabilities.rs`

## Summary

| Metric | Value |
|---|---:|
| Total tests | 229 |
| Passed | 229 |
| Failed | 0 |
| Skipped | 0 |
| Shared contract (`scope=shared`) | 179 |
| Shared parity status | Pass (1:1 with Node/Python) |
| Rust extension tests (`scope=extension`) | 50 |

## Scope Model

- Shared capabilities: governed by `examples-test/capability-contract.yaml`.
  - CID format: `CID-SHARED-xxx`
  - Blocking policy: all shared CIDs are blocking.
- Rust extension capabilities (not counted into shared 1:1):
  - direct `ReadTxn` / `DbSnapshot`
  - `execute_mixed` / `ExecuteOptions`
  - reify internals and low-level storage/maintenance coverage

## Gate Status

- `bash scripts/parity_softgate_audit.sh`: Pass
- `bash scripts/parity_coverage_audit.sh`: Pass
- `bash scripts/binding_parity_gate.sh`: Pass
- `bash examples-test/run_all.sh`: Pass

## Notes

- This report separates "shared parity" and "extension coverage" explicitly.
- Shared parity means same CID, same expected mode, and all three bindings have corresponding tests.
