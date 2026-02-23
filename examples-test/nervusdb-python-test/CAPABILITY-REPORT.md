# NervusDB Python Binding — Capability Test Report

> Updated: 2026-02-23
> Test entry: `examples-test/nervusdb-python-test/test_capabilities.py`

## Summary

| Metric | Value |
|---|---:|
| Total tests | 214 |
| Passed | 214 |
| Failed | 0 |
| Skipped | 0 |
| Shared contract (`scope=shared`) | 179 |
| Shared parity status | Pass (1:1 with Rust/Node) |
| Extension tests (`scope=extension`) | 35 |

## Scope Model

- Shared capabilities: governed by `examples-test/capability-contract.yaml`.
  - CID format: `CID-SHARED-xxx`
  - Blocking policy: all shared CIDs are blocking.
- Python extension capabilities (not counted into shared 1:1):
  - `query_stream`
  - typed objects (`Node` / `Relationship` / `Path`)
  - exception hierarchy and Python-specific boundary behavior
  - Python-specific API ergonomics

## Gate Status

- `bash scripts/parity_softgate_audit.sh`: Pass
- `bash scripts/parity_coverage_audit.sh`: Pass
- `bash scripts/binding_parity_gate.sh`: Pass
- `bash examples-test/run_all.sh`: Pass

## Notes

- This report separates "shared parity" and "extension coverage" explicitly.
- Shared parity means same CID, same expected mode, and all three bindings have corresponding tests.
