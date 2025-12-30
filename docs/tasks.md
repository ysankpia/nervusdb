# Task Tracking (Rust-First 1.0)

| ID | Task | Risk | Status | Branch | Notes |
|----|------|------|--------|--------|-------|
| T61 | [Example] Implement `examples/tour.rs` to validate DX | Low | Done | - | A comprehensive tour of CRUD & Cypher |
| T62 | [API] Audit `nervusdb-v2` exports & naming | Low | Done | - | Fixed dependency cycle & re-exports |
| T63 | [CLI] Implement REPL mode in `nervusdb-cli` | Medium | Done | feat/T63-cli-repl | Interactive shell support |
| T64 | [Docs] Generate & Polish RustDocs | Low | Done | feat/T64-rustdocs | Ensure `cargo doc --open` looks professional |
| T65 | Query Engine: Support String Labels/Types | High | Done | feat/T65-string-labels | Verified in tour.rs, `MATCH (n:Person)` support |
| T66 | [Test] Resilience/Persistence Verification | High | Done | - | Verified label persistence in `tests/resilience_labels.rs` |
| T67 | [API] Finalize Public Facade & Hide Internals | Medium | Plan | - | Encapsulate `InternalId`, clean `pub` exports |

## Status Definitions

- Plan: Planned
- WIP: Work in Progress
- Review: Pending Review
- Done: Completed
- Blocked: Blocked
