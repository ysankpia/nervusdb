# Task Tracking (v2.0 Roadmap)

> **Focus**: Architecture Parity (Indexes), Cypher Completeness, and Ecosystem.
> **Source**: `docs/ROADMAP_2.0.md`

| ID            | Task                                                  | Risk   | Status | Branch                      | Notes                                        |
| :------------ | :---------------------------------------------------- | :----- | :----- | :-------------------------- | :------------------------------------------- |
| **Phase 1**   | **Core Architecture**                                 |        |        |                             |                                              |
| T101          | [Storage] Implement `PageCursor` & B-Tree Page Layout | High   | Done   | -                           | Slotted pages + ordered keys + cursor        |
| T102          | [Storage] Implement `IndexCatalog` & B-Tree Logic     | High   | Done   | -                           | Insert/Search/Delete on Pager                |
| T103          | [Storage] Compaction Integration (Merge to Index)     | High   | Done   | -                           | Prevent property loss on checkpoint          |
| T104          | [Query] Implement `EXPLAIN` Clause                    | Low    | Done   | -                           | Show Plan visualization                      |
| T105          | [Query] Implement `MERGE` Clause                      | Medium | Done   | -                           | Idempotent Create                            |
| T106          | [Lifecycle] Implement Checkpoint-on-Close             | Medium | Done   | -                           | Merge WAL to NDB on shutdown                 |
| T107          | [Query] Index Integration (Optimizer V1)              | High   | Done   | feat/T107-index-integration | Connect Query to Storage IndexCatalog        |
| T108          | [Query] Implement `SET` Clause (Updates)              | High   | Done   | feat/T108-set-clause        | Enable property updates (WAL+Index)          |
| **Phase 1.5** | **Production Hardening (Gap Filling)**                |        |        |                             |                                              |
| T151          | [Query] Implement `OPTIONAL MATCH` (Left Join)        | High   | Done   | feat/T151-optional-match    | Core graph pattern support                   |
| T152          | [Query] Implement Aggregation Functions (COLLECT/MIN) | Medium | Done   | feat/T152-aggregation       | Extended executor capabilities               |
| T153          | [Query] VarLen Optional Match (Chaining)              | Medium | Done   | feat/T152-aggregation       | Handled in Gap Filling Phase                 |
| T154          | [Storage] Support Complex Types (Date/Map/List)       | High   | Plan   | -                           | Extend PropertyValue & Serialization         |
| T155          | [Storage] Implement Overflow Pages (Large Blobs)      | High   | Done   | -                           | Support properties > 8KB                     |
| T156          | [Query] Optimizer V2 (Statistics & CBO Basics)        | High   | Done   | -                           | Histogram-based index selection              |
| T157          | [Tool] Implement Offline Bulk Loader                  | High   | Done   | -                           | Direct SST/Page generation                   |
| T158          | [Lifecycle] Online Backup API                         | Medium | Done   | feat/T158-online-backup     | Hot snapshot capability                      |
| **Phase 2**   | **v2.0.0 Stable Release Preparation**                 |        |        |                             |                                              |
| T159          | [Release] v2.0.0 发布准备 (crates.io + 文档 + Binary) | High   | Done   | -                           | crates.io 5 个 crate 已发布 + GitHub Release |
| T160          | [Docs] 完善 README 和 User Guide                      | Medium | Done   | -                           | Slogan + 快速上手 + 特性表格                 |
| T161          | [Release] GitHub Releases 二进制分发                  | Medium | Done   | -                           | Linux binary 已发布                          |
| T162          | [Benchmark] 性能基准测试和公布                        | Medium | Done   | feat/T162-benchmark         | 5 万/10 万节点测试结果已保存                 |
| T163          | [CI] 自动化 Release CI                                | Medium | Done   | -                           | Tag 触发自动发布 Linux/macOS/Windows binary  |
| T201          | [Binding] UniFFI Setup & Python Binding               | Medium | Done   | feat/T201-python-binding    | `pip install nervusdb`                       |
| T202          | [Tool] Bulk Import Tool (CSV/JSONL)                   | Medium | Plan   | -                           | Bypass WAL for speed                         |
| T203          | [AI] HNSW Index Prototype                             | High   | Plan   | -                           | Vector Search MVP                            |

## Archived (v1/Alpha)

_Previous tasks (T1-T67) are archived in `docs/memos/DONE.md`._
