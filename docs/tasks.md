# Task Tracking (v2.0 Roadmap)

> **Focus**: Architecture Parity (Indexes), Cypher Completeness, and Ecosystem.
> **Source**: `docs/ROADMAP_2.0.md`

| ID | Task | Risk | Status | Branch | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Phase 1** | **Core Architecture** | | | | |
| T101 | [Storage] Implement `PageCursor` & B-Tree Page Layout | High | Done | - | Slotted pages + ordered keys + cursor |
| T102 | [Storage] Implement `IndexCatalog` & B-Tree Logic | High | WIP | feat/T102-index-catalog | Insert/Search/Delete on Pager |
| T103 | [Storage] Compaction Integration (Merge to Index) | High | Plan | - | Feed MemTable data into B-Tree on flush |
| T104 | [Query] Implement `EXPLAIN` Clause | Low | Plan | - | Show Plan visualization |
| T105 | [Query] Implement `MERGE` Clause | Medium | Plan | - | Idempotent Create |
| T106 | [Lifecycle] Implement Checkpoint-on-Close | Medium | Plan | - | Merge WAL to NDB on shutdown |
| **Phase 2** | **Ecosystem & AI** | | | | |
| T201 | [Binding] UniFFI Setup & Python Binding | Medium | Plan | - | `pip install nervusdb` |
| T202 | [Tool] Bulk Import Tool (CSV/JSONL) | Medium | Plan | - | Bypass WAL for speed |
| T203 | [AI] HNSW Index Prototype | High | Plan | - | Vector Search MVP |

## Archived (v1/Alpha)
*Previous tasks (T1-T67) are archived in `docs/memos/DONE.md`.*
