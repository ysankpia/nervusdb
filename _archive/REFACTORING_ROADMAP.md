# NervusDB 重构路线图：成为"图数据库界的 SQLite"

## 当前架构诊断

### 核心问题：精神分裂架构 (Split-Brain Architecture)

当前项目存在 **"胖 Node 绑定，瘦 Rust 核心"** 的问题：

| 功能模块 | 当前位置 | 问题 |
|---------|---------|------|
| 图算法 (PageRank, Dijkstra) | TypeScript | Python 用户无法使用，性能差 |
| 全文检索 (TF-IDF, BM25) | TypeScript | 无法跨语言复用 |
| 空间索引 (R-Tree) | TypeScript | 性能瓶颈 |
| Cypher 解析器 | **TS + Rust 双重实现** | 维护噩梦 |
| 时间记忆 | TS + Rust 部分重叠 | 代码混乱 |

### SQLite 的正确模式

```
┌─────────────────────────────────────────────────────────┐
│                    User Application                      │
└─────────────────────────────────────────────────────────┘
         │              │              │              │
         ▼              ▼              ▼              ▼
    ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐
    │ Node.js │   │ Python  │   │   C/C++ │   │  WASM   │
    │ Binding │   │ Binding │   │ Binding │   │ Binding │
    │ (~500行) │   │ (~200行) │   │  (FFI)  │   │ (直接) │
    └────┬────┘   └────┬────┘   └────┬────┘   └────┬────┘
         │              │              │              │
         └──────────────┴──────────────┴──────────────┘
                                 │
                                 ▼
    ┌─────────────────────────────────────────────────────┐
    │              nervusdb-core (Rust)                    │
    │  ┌─────────────────────────────────────────────┐    │
    │  │  Hexastore Storage (redb)                   │    │
    │  │  Cypher Parser & Executor                   │    │
    │  │  Graph Algorithms (PageRank, Dijkstra...)   │    │
    │  │  Full-Text Search (Inverted Index, BM25)   │    │
    │  │  Spatial Index (R-Tree)                     │    │
    │  │  Temporal Memory                            │    │
    │  └─────────────────────────────────────────────┘    │
    └─────────────────────────────────────────────────────┘
```

---

## 重构阶段

### Phase 1: 清理与归档 ✅ 完成

已将以下 TypeScript 实现归档到 `_archive/` 目录：

- [x] `ts-algorithms/` - 图算法 (8 文件)
- [x] `ts-fulltext/` - 全文检索 (10 文件)
- [x] `ts-spatial/` - 空间索引 (4 文件)
- [x] `ts-query-pattern/` - TS Cypher 解析器 (8 文件)
- [x] `ts-temporal-memory/` - 时间记忆 (3 文件)
- [x] `ts-tests/` - 对应的测试文件 (41 文件)

**共计 74 个 TypeScript 文件已归档**

### Phase 2: 精简 Node Binding (待执行)

目标：将 `bindings/node/src/` 从 ~50 个文件精简到 ~10 个文件

#### 2.1 删除已归档模块的原文件

```bash
rm -rf bindings/node/src/extensions/algorithms/
rm -rf bindings/node/src/extensions/fulltext/
rm -rf bindings/node/src/extensions/spatial/
rm -rf bindings/node/src/extensions/query/pattern/
rm -rf bindings/node/src/memory/
```

#### 2.2 简化 extensions/index.ts

只保留必要的查询扩展（path/, aggregation, cypher, iterator）

#### 2.3 简化 synapseDb.ts

移除对已删除模块的引用

### Phase 3: 增强 Rust Core (待执行)

#### 3.1 图算法实现 (参考 `_archive/ts-algorithms/`)

```rust
// nervusdb-core/src/algorithms/mod.rs
pub mod centrality;  // PageRank, Betweenness, Closeness
pub mod community;   // Louvain, Label Propagation
pub mod pathfinding; // Dijkstra, A*, BFS
pub mod similarity;  // Jaccard, Cosine
```

#### 3.2 全文检索实现 (参考 `_archive/ts-fulltext/`)

```rust
// nervusdb-core/src/fulltext/mod.rs
pub mod analyzer;      // Tokenizer, Stemmer
pub mod inverted_index;
pub mod scorer;        // TF-IDF, BM25
pub mod search_engine;
```

#### 3.3 空间索引实现 (参考 `_archive/ts-spatial/`)

```rust
// nervusdb-core/src/spatial/mod.rs
pub mod rtree;
pub mod geometry;
pub mod spatial_query;
```

### Phase 4: 更新 FFI 接口

```rust
// nervusdb-core/src/ffi.rs 新增
#[no_mangle]
pub extern "C" fn nervus_pagerank(db: *mut Database, ...) -> ...
#[no_mangle]
pub extern "C" fn nervus_dijkstra(db: *mut Database, ...) -> ...
#[no_mangle]
pub extern "C" fn nervus_fulltext_search(db: *mut Database, ...) -> ...
#[no_mangle]
pub extern "C" fn nervus_spatial_query(db: *mut Database, ...) -> ...
```

### Phase 5: 更新语言绑定

#### Node.js (N-API)

```rust
// bindings/node/native/nervusdb-node/src/lib.rs
#[napi]
fn pagerank(env: Env, db: &Database, ...) -> Result<...>
#[napi]
fn dijkstra(env: Env, db: &Database, ...) -> Result<...>
```

#### Python (PyO3)

```rust
// bindings/python/nervusdb-py/src/lib.rs
#[pyfunction]
fn pagerank(db: &DatabaseHandle, ...) -> PyResult<...>
#[pyfunction]
fn dijkstra(db: &DatabaseHandle, ...) -> PyResult<...>
```

---

## 优先级排序

| 优先级 | 功能 | 难度 | 影响 |
|-------|------|------|------|
| P0 | 删除 TS 冗余代码 | 低 | 清理代码库 |
| P1 | 完善 Rust Cypher Executor | 中 | 核心查询能力 |
| P1 | Rust Dijkstra/BFS | 中 | 最常用路径算法 |
| P2 | Rust PageRank | 中 | 图分析能力 |
| P2 | Rust 倒排索引 | 高 | 全文检索 |
| P3 | Rust R-Tree | 高 | 空间查询 |
| P3 | Rust Louvain | 高 | 社区发现 |

---

## 代码行数对比预期

| 模块 | 当前 (TS + Rust) | 目标 (Rust Only) |
|------|-----------------|-----------------|
| nervusdb-core | ~3,000 行 | ~8,000 行 |
| bindings/node | ~15,000 行 | ~1,500 行 |
| bindings/python | ~150 行 | ~500 行 |

**总体目标**: 绑定层代码量减少 90%，核心层增加 150%

---

## 参考资料

- SQLite 架构: https://www.sqlite.org/arch.html
- Linus Torvalds on "Good Taste": https://www.youtube.com/watch?v=o8NPllzkFhE
- redb (Rust embedded database): https://github.com/cberner/redb
