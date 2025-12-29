# Task Tracking (Rust-First 1.0)

| ID | Task | Risk | Status | Branch | Notes |
|----|------|------|--------|--------|-------|
| T61 | [Example] Implement `examples/tour.rs` to validate DX | Low | Done | - | A comprehensive tour of CRUD & Cypher |
| T62 | [API] Audit `nervusdb-v2` exports & naming | Low | Done | - | Fixed dependency cycle & re-exports |
| T63 | [CLI] Implement REPL mode in `nervusdb-cli` | Medium | Done | - | Interactive shell support |
| T64 | [Docs] Generate & Polish RustDocs | Low | Done | feat/T64-rustdocs | Ensure `cargo doc --open` looks professional |
| T65 | [Query] Support String Labels/Types in Cypher | High | WIP | feat/T65-string-labels | `MATCH (n:Person)` support (Resolve M3 limitation) |
| T66 | [Test] Resilience/Persistence Verification | High | Plan | - | Ensure LabelInterner survives restart/kill |
| T67 | [API] Finalize Public Facade & Hide Internals | Medium | Plan | - | Encapsulate `InternalId`, clean `pub` exports |

## Status Definitions
- Plan: Planned
- WIP: Work in Progress
- Review: Pending Review
- Done: Completed
- Blocked: Blocked
