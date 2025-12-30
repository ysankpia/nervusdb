# Task Tracking (v2.0 Roadmap)

> **Focus**: Architecture Parity (Indexes), Cypher Completeness, and Ecosystem.
> **Source**: `docs/ROADMAP_2.0.md`

| ID | Task | Risk | Status | Branch | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Phase 1** | **Core Architecture** | | | | |
| T101 | [Storage] Implement `PageCursor` & B-Tree Page Layout | High | Done | - | Slotted pages + ordered keys + cursor |
| T102 | [Storage] Implement `IndexCatalog` & B-Tree Logic | High | Done | - | Insert/Search/Delete on Pager |
| T103 | [Storage] Compaction Integration (Merge to Index) | High | Done | - | Prevent property loss on checkpoint |
| T104 | [Query] Implement `EXPLAIN` Clause | Low | Done | - | Show Plan visualization |
| T105 | [Query] Implement `MERGE` Clause | Medium | WIP | feat/T105-merge | Idempotent Create |
| T106 | [Lifecycle] Implement Checkpoint-on-Close | Medium | Plan | - | Merge WAL to NDB on shutdown |
| **Phase 2** | **Ecosystem & AI** | | | | |
| T201 | [Binding] UniFFI Setup & Python Binding | Medium | Plan | - | `pip install nervusdb` |
| T202 | [Tool] Bulk Import Tool (CSV/JSONL) | Medium | Plan | - | Bypass WAL for speed |
| T203 | [AI] HNSW Index Prototype | High | Plan | - | Vector Search MVP |

## Archived (v1/Alpha)
*Previous tasks (T1-T67) are archived in `docs/memos/DONE.md`.*
