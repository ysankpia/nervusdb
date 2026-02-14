# NervusDB v2 架构文档

> Rust-native, crash-safe embedded property graph database.
> 目标：图数据库界的 SQLite

---

# 第一部分：现状架构

## 1. 项目概述

NervusDB v2 是一个 Rust 原生的嵌入式属性图数据库，采用 `.ndb`（数据）+ `.wal`（日志）双文件存储。Cargo.toml 版本 2.0.0，当前处于 SQLite-Beta 收敛阶段，核心图操作可用，Cypher 支持持续推进中。

```
当前状态:
- 版本: Cargo.toml 2.0.0 / SQLite-Beta 收敛版
- 存储格式: .ndb + .wal（storage_format_epoch 强校验，不匹配报 StorageFormatMismatch）
- 页面大小: 8KB (硬编码)
- 节点ID: u32 (最多 ~42亿)
- 边标识: (src, rel, dst) 三元组
- TCK 通过率: 81.93% (3193/3897, Tier-3 全量, 2026-02-11)
- NotImplemented: 9 (executor 6 + query_api 2 + parser 1)

Beta 硬门槛（必须同时满足）:
- 官方全量 openCypher TCK 通过率 ≥95%（Tier-3 全量口径）
- warnings 视为阻断（fmt/clippy/tests/bindings 链路）
- 冻结阶段连续 7 天主 CI + nightly 稳定
```

## 2. Workspace 结构

```
nervusdb/                              当前目录名 (带 -v2)          发布目标
├── nervusdb-api/       # Layer 0   → nervusdb-api              crates.io (内部)
├── nervusdb-storage/   # Layer 1   → nervusdb-storage          crates.io (内部)
├── nervusdb-query/     # Layer 2   → nervusdb-query            crates.io (内部)
├── nervusdb/           # Layer 3   → nervusdb                  crates.io (对外)
├── nervusdb-cli/          # CLI       → nervusdb-cli              crates.io (对外)
├── nervusdb-pyo3/         # Python    → nervusdb (PyPI)           Rust 稳定后发布
├── nervusdb-node/         # Node.js   → nervusdb (npm)            Rust 稳定后发布
│                          # [独立 workspace，不在根 Cargo.toml 中]
├── fuzz/                  # Fuzzing   → nervusdb-fuzz             独立 workspace，依赖 nervusdb-query
└── scripts/               # 脚本      — TCK 门控、基准测试、发布等
```

> 所有 `-v2` 后缀在 Phase 1a 包名收敛时统一去掉（目录名 + Cargo.toml package name）。

依赖关系（收敛后）：

```
nervusdb (Facade)
  ├── nervusdb-api      (trait 定义)
  ├── nervusdb-storage  (存储引擎) ── depends on ── nervusdb-api
  └── nervusdb-query    (查询引擎) ── depends on ── nervusdb-api
```

## 2.5 发布策略

### 开发阶段（当前）

当前 crate 名仍带 `-v2` 后缀，内部开发使用。Phase 1a 包名收敛时统一去掉 `-v2`，同时：
- 去掉代码中残留的版本数字（如 `nervusdb_api::` → `nervusdb_api::`）
- TCK 测试文件名中的 `tXXX_` 数字前缀（如 t155_edge_persistence, t306_unwind）在 TCK 通过率达到 100% 后统一重构为语义化命名

### 发布阶段（Rust 稳定后）

对外只发布 2 个 crate，用户只需关心：

| 包名 | 用途 | 发布目标 |
|------|------|----------|
| `nervusdb` | 嵌入式图数据库库 | crates.io |
| `nervusdb-cli` | 命令行工具 | crates.io |

内部子包（nervusdb-api / nervusdb-storage / nervusdb-query）发布到 crates.io 但标记为内部实现细节，不保证 API 稳定性。

多语言绑定在 Rust 端 API 稳定后发布到各自包管理器：

| 绑定 | 包管理器 | 包名 | 前置条件 |
|------|----------|------|----------|
| Python (PyO3) | PyPI | `nervusdb` | Rust API 稳定 |
| Node.js (N-API) | npm | `nervusdb` | Rust API 稳定 |

### 设计原则（目标态，当前尚未完全实现）

- 主包 `nervusdb` 完整 re-export 所有公共 API（包括 `GraphStore` trait、`vacuum`、`backup` 等）
  - **当前缺口**：`GraphStore` 只有 `use` 没有 `pub use`（lib.rs:50）；vacuum/backup/bulkload 模块未 re-export
- CLI 只依赖 `nervusdb` 主包，不直接引用内部子包
  - **当前缺口**：main.rs:6 和 repl.rs:5 直接引用 `nervusdb-storage`
- 用户 `cargo add nervusdb` 即可获得全部功能，无需了解内部分包

## 3. 整体架构

```
┌──────────────────────────────────────────────────────┐
│  Language Bindings                                   │
│  Python (PyO3) │ Node.js (N-API)                     │
├──────────────────────────────────────────────────────┤
│  nervusdb (Facade)                                │
│  Db │ ReadTxn │ WriteTxn │ DbSnapshot                │
├──────────────────────────────────────────────────────┤
│  nervusdb-query (查询引擎)                         │
│  Lexer → Parser → AST → query_api → Executor         │
├──────────────────────────────────────────────────────┤
│  nervusdb-storage (存储引擎)                       │
│  WAL │ MemTable │ L0Run │ CSR │ Pager │ Index        │
├──────────────────────────────────────────────────────┤
│  nervusdb-api (类型 + Trait)                       │
│  GraphStore │ GraphSnapshot │ PropertyValue │ EdgeKey │
├──────────────────────────────────────────────────────┤
│  OS (pread/pwrite, fsync)                            │
└──────────────────────────────────────────────────────┘
```

## 4. 存储引擎 (nervusdb-storage)

### 4.1 架构概览：LSM-Tree 变体

```
Write Path:
  Client → WAL (fsync) → MemTable → commit → L0Run (内存)
                                                  ↓ compact()
                                            CsrSegment (磁盘 .ndb)

Read Path:
  Client → Snapshot
             ├── L0Run[] (newest first, 内存)
             ├── CsrSegment[] (磁盘)
             └── B-Tree Property Store (磁盘)
```

### 4.2 Pager (页面管理器)

```rust
// nervusdb-storage/src/pager.rs
const PAGE_SIZE: usize = 8192;  // 8KB 硬编码

文件布局:
  Page 0: Meta
    [0..16]   FILE_MAGIC (16 bytes)
    [16..20]  version_major: u32
    [20..24]  version_minor: u32
    [24..32]  page_size: u64
    [32..40]  bitmap_page_id: u64
    [40..48]  next_page_id: u64
    [48..56]  i2e_start_page_id: u64
    [56..64]  i2e_len: u64
    [64..72]  next_internal_id: u64
    [72..80]  index_catalog_root: u64
    [80..84]  next_index_id: u32
    [84..92]  storage_format_epoch: u64
    [92..8192] 保留 (全零)
  Page 1: Bitmap (空闲页位图, 单页 → 最多 65536 页 = 512MB)
  Page 2+: 数据页 (B-Tree 节点, CSR 段, Blob 等)
```

特点：
- 直接使用 OS 的 `pread`/`pwrite`，无页面缓存
- 单 Bitmap 页管理空闲页，容量上限 512MB
- Meta 页存储全局元数据（版本、ID 计数器、索引根等）

### 4.3 WAL (Write-Ahead Log)

```rust
// nervusdb-storage/src/wal.rs
enum WalRecord {
    BeginTx { txid: u64 },
    CommitTx { txid: u64 },
    PageWrite { page_id: u64, page: Box<[u8; 8192]> },
    PageFree { page_id: u64 },
    CreateLabel { name: String, label_id: u32 },
    CreateNode { external_id: u64, label_id: u32, internal_id: u32 },
    AddNodeLabel { node: u32, label_id: u32 },
    RemoveNodeLabel { node: u32, label_id: u32 },
    CreateEdge { src: u32, rel: u32, dst: u32 },
    TombstoneNode { node: u32 },
    TombstoneEdge { src: u32, rel: u32, dst: u32 },
    ManifestSwitch { epoch, segments, properties_root, stats_root },
    Checkpoint { up_to_txid, epoch, properties_root, stats_root },
    SetNodeProperty { node, key, value },
    SetEdgeProperty { src, rel, dst, key, value },
    RemoveNodeProperty { node, key },
    RemoveEdgeProperty { src, rel, dst, key },
}
```

特点：
- CRC32 校验每条记录
- 混合逻辑记录（CreateNode）和物理记录（PageWrite）
- 事务边界（BeginTx/CommitTx）
- 恢复时只重放已提交事务
- checkpoint_on_close 在关闭时压缩 WAL

### 4.4 MemTable (写缓冲)

```rust
// nervusdb-storage/src/memtable.rs
struct MemTable {
    out: HashMap<InternalNodeId, Vec<EdgeKey>>,      // 出边
    in_: HashMap<InternalNodeId, Vec<EdgeKey>>,      // 入边
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    node_properties: HashMap<InternalNodeId, HashMap<String, PropertyValue>>,
    edge_properties: HashMap<EdgeKey, HashMap<String, PropertyValue>>,
    removed_node_properties: HashMap<InternalNodeId, BTreeSet<String>>,
    removed_edge_properties: HashMap<EdgeKey, BTreeSet<String>>,
}
```

写事务提交时，MemTable 冻结为不可变的 L0Run。

### 4.5 L0Run (内存快照)

```rust
// nervusdb-storage/src/snapshot.rs
struct L0Run {
    txid: u64,
    edges_by_src: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    edges_by_dst: BTreeMap<InternalNodeId, Vec<EdgeKey>>,
    tombstoned_nodes: BTreeSet<InternalNodeId>,
    tombstoned_edges: BTreeSet<EdgeKey>,
    node_properties: BTreeMap<InternalNodeId, BTreeMap<String, PropertyValue>>,
    edge_properties: BTreeMap<EdgeKey, BTreeMap<String, PropertyValue>>,
    tombstoned_node_properties: BTreeMap<InternalNodeId, BTreeSet<String>>,
    tombstoned_edge_properties: BTreeMap<EdgeKey, BTreeSet<String>>,
}
```

L0Run 是 MemTable 的不可变快照，保留在内存中直到 compact。

### 4.6 CsrSegment (磁盘段)

```rust
// nervusdb-storage/src/csr.rs
struct CsrSegment {
    id: SegmentId,
    meta_page_id: u64,
    min_src / max_src / min_dst / max_dst: InternalNodeId,
    offsets: Vec<u64>,          // CSR 行偏移（出边）
    edges: Vec<EdgeRecord>,     // 出边数组 { rel: u32, dst: u32 }
    in_offsets: Vec<u64>,       // CSR 行偏移（入边）
    in_edges: Vec<EdgeRecord>,  // 入边数组
    // 注意: in_edges 中 EdgeRecord 的 dst 字段实际存储的是源节点 ID
    // incoming_neighbors() 做了语义反转: EdgeKey { src: e.dst, rel: e.rel, dst } (csr.rs:74-78)
}
```

CSR（Compressed Sparse Row）格式，compact 时从 L0Run 构建并持久化到 .ndb。

### 4.7 属性存储

两层结构：
1. L0Run 中的内存属性（未压缩的最新数据）
2. B-Tree Property Store（compact 后的持久化数据）

compact 时属性 sink 到 B-Tree：
- 键编码：`[tag:1][node_id:4][key_len:4][key_bytes]`
- 值通过 BlobStore 存储编码后的 PropertyValue

### 4.8 GraphEngine (引擎核心)

```rust
// nervusdb-storage/src/engine.rs
struct GraphEngine {
    pager: Arc<RwLock<Pager>>,
    wal: Mutex<Wal>,
    idmap: Mutex<IdMap>,
    label_interner: Mutex<LabelInterner>,
    index_catalog: Arc<Mutex<IndexCatalog>>,
    vector_index: Arc<Mutex<HnswIndex>>,
    published_runs: RwLock<Arc<Vec<Arc<L0Run>>>>,
    published_segments: RwLock<Arc<Vec<Arc<CsrSegment>>>>,
    published_labels: RwLock<Arc<LabelSnapshot>>,
    published_node_labels: RwLock<Arc<Vec<Vec<LabelId>>>>,
    write_lock: Mutex<()>,
    next_txid: AtomicU64,
    ...
}
```

### 4.9 StorageSnapshot (桥接层)

```rust
// nervusdb-storage/src/api.rs
struct StorageSnapshot {
    inner: snapshot::Snapshot,           // 内部快照（L0Run + CsrSegment，无锁 Arc clone）
    i2e: Arc<Vec<I2eRecord>>,            // 全量 i2e 拷贝（snapshot() 时 O(N) 扫描）
    tombstoned_nodes: Arc<HashSet<InternalNodeId>>,
    pager: Arc<RwLock<Pager>>,           // 读属性时需要 Pager 读锁
    index_catalog: Arc<Mutex<IndexCatalog>>, // lookup_index() 需要锁
    stats_cache: Mutex<Option<GraphStatistics>>,
}
```

属性读取的两层路径：
1. L0Run 内存属性 → 直接读取，无锁
2. B-Tree 持久化属性 → 需要 `self.pager.read().unwrap()` 获取 Pager 读锁

类型转换函数：
- `conv()`: `snapshot::EdgeKey` → `nervusdb_api::EdgeKey`（字段相同，类型不同）
- `convert_property_value()`: `storage::PropertyValue` → `api::PropertyValue`（逐变体映射）
- `to_storage()`: 反向转换

### 4.10 辅助子系统

| 模块 | 文件 | 职责 |
|------|------|------|
| backup | backup.rs | 在线备份 API，拷贝 .ndb 文件 |
| bulkload | bulkload.rs | 离线批量加载器，绕过 WAL 直接写入 |
| vacuum | vacuum.rs | 就地 vacuum：标记可达页面后重写 .ndb |
| stats | stats.rs | GraphStatistics（compact 时收集节点/边计数） |
| blob_store | blob_store.rs | 大值存储，4KB 页面链 |
| property | property.rs (11K) | 属性值编码/解码 |
| error | error.rs (1K) | 错误类型定义 |
| idmap | idmap.rs (8.2K) | ExternalId ↔ InternalNodeId 映射 |
| label_interner | label_interner.rs (8.3K) | 标签名 ↔ LabelId 映射 |

### 5.1 处理流水线

```
Cypher String → Lexer → Parser → AST → query_api::prepare() → PreparedQuery
                                                                    ↓
                                                    executor::execute_plan()
                                                                    ↓
                                                              Row Stream
```

### 5.2 AST (完整 Cypher 语法树)

```rust
// nervusdb-query/src/ast.rs
enum Clause {
    Match, Create, Merge, Unwind, Call, Return,
    Where, With, Set, Remove, Delete, Union, Foreach,
}

enum Expression {
    Literal, Variable, PropertyAccess, Binary, Unary,
    FunctionCall, Case, Exists, List, ListComprehension,
    Map, Parameter,
}
```

### 5.3 文件规模

| 文件 | 大小 | 职责 |
|------|------|------|
| executor.rs | ~242K chars | 所有执行逻辑 |
| evaluator.rs | ~166K chars | 表达式求值 |
| query_api.rs | ~153K chars | 查询准备和计划 |
| parser.rs | ~58K chars | Cypher 解析器 |
| lexer.rs | ~19K chars | 词法分析器 |
| ast.rs | 8.5K chars | AST 定义 |
| facade.rs | ~3K chars | 查询门面（QueryExt） |
| parser_helper_exists.rs | ~1.4K chars | EXISTS 子查询解析辅助 |
| error.rs | ~708B | 错误类型定义 |

## 6. 并发模型

```
写事务: write_lock (Mutex) → 全局互斥
读事务: begin_read() → Arc clone published_* → 无锁创建内部 Snapshot
         snapshot() → 锁 idmap 全量拷贝 i2e → 创建 StorageSnapshot（持有 Pager/IndexCatalog 锁引用）

注意: begin_read() 创建的内部 Snapshot 确实是无锁的（Arc clone），
但 GraphStore::snapshot() 创建的 StorageSnapshot 需要锁 idmap 做 O(N) 拷贝，
且后续读属性需要 Pager 读锁，lookup_index() 需要 IndexCatalog 锁。

锁清单:
  write_lock: Mutex<()>              — 写事务互斥
  pager: Arc<RwLock<Pager>>          — 页面读写
  wal: Mutex<Wal>                    — WAL 追加
  idmap: Mutex<IdMap>                — ID 映射
  label_interner: Mutex<LabelInterner> — 标签字典
  index_catalog: Arc<Mutex<IndexCatalog>> — 索引目录
  vector_index: Arc<Mutex<HnswIndex>>    — 向量索引
```

## 7. 索引系统

| 索引类型 | 实现 | 状态 |
|----------|------|------|
| B-Tree 属性索引 | btree.rs | 可用，不支持回填 |
| HNSW 向量索引 | hnsw/ | 可用 |
| 标签索引 | 无 | 缺失 |
| 全文索引 | 无 | 缺失 |

## 8. 数据模型

```rust
// nervusdb-api/src/lib.rs
type ExternalId = u64;       // 用户指定的节点 ID
type InternalNodeId = u32;   // 内部自增 ID
type LabelId = u32;          // 标签 ID
type RelTypeId = u32;        // 关系类型 ID

struct EdgeKey {
    src: InternalNodeId,     // 源节点
    rel: RelTypeId,          // 关系类型
    dst: InternalNodeId,     // 目标节点
}
// 注意: (src, rel, dst) 作为唯一键 → 不支持多重边

enum PropertyValue {
    Null, Bool(bool), Int(i64), Float(f64), String(String),
    DateTime(i64), Blob(Vec<u8>), List(Vec<PropertyValue>),
    Map(BTreeMap<String, PropertyValue>),
}
```

## 9. 现状评估

### 9.1 优势

| 优势 | 说明 |
|------|------|
| 清晰的 crate 分层 | api → storage → query → facade，依赖方向正确 |
| LSM-Tree 变体 | MemTable → L0Run → CSR，适合写密集场景 |
| WAL 崩溃安全 | CRC32 校验，事务边界，replay 恢复 |
| CSR 格式 | 紧凑边存储，高效邻居遍历 |
| HNSW 向量索引 | 支持向量相似度搜索（差异化特性） |
| 完整 Cypher AST | 语法树覆盖大部分 Cypher 语法 |
| 丰富的测试基础设施 | TCK / fuzz / chaos / soak / benchmark |
| 多语言绑定 | Python (PyO3) + Node.js (N-API) |

### 9.2 问题清单

| 级别 | 问题 | 影响 |
|------|------|------|
| P0 | executor.rs ~242K chars 单文件 | 不可维护 |
| P0 | 没有查询优化器 / Plan 层 | 查询性能差，无法优化 |
| P0 | 没有页面缓存 (Buffer Pool) | 每次 B-Tree 操作直接 I/O |
| P1 | 9 个 NotImplemented (executor 6 + query_api 2 + parser 1) | Cypher 功能不完整 |
| P1 | 索引崩溃安全性缺陷 | 索引更新在 WAL fsync 前写入 .ndb，崩溃后索引含幽灵条目，无重建机制 |
| P1 | snapshot() O(N) 瓶颈 | scan_i2e_records() 全量拷贝 i2e 表，锁 idmap (Mutex) 阻塞写事务 |
| P1 | index_catalog 锁竞争 | StorageSnapshot 持有 Arc<Mutex<IndexCatalog>>，lookup_index() 需要锁 |
| P1 | PropertyValue / EdgeKey 重复定义 | 大量转换代码 |
| P1 | StorageSnapshot 持有 Pager 锁 | 读写互相阻塞 |
| P1 | 边不支持多重边 | 图语义不完整 |
| P1 | 没有标签索引 | 标签扫描 O(N) |
| P1 | 索引不支持回填 | 创建索引后旧数据不可查 |
| P1 | CSR 段没有合并策略 | 段越多读越慢 |
| P2 | 没有 VFS 抽象层 | 不可测试、不可扩展 |
| P2 | B-Tree delete 不回收页面 | delete() 只删除单元格不合并页面，频繁 insert/delete 导致页面碎片化 |
| P2 | IndexCatalog 单页存储限制 | 所有索引定义存储在一个 8KB 页面中，约 250 个索引上限 |
| P2 | 单 Bitmap 页限制 512MB | 大图受限（→ 12.5 Overflow Bitmap 方案，扩至 ~506GB） |
| P2 | 页面大小硬编码 8KB | 不可配置 |
| P2 | 没有页面校验和 | 数据损坏不可检测 |
| P2 | 属性键无字符串字典 | 内存和存储浪费 |
| P1 | CLI 直接引用 nervusdb-storage（GraphEngine + vacuum_in_place），绕过 Db 门面层 | CLI 的 main.rs（line 6: GraphEngine, line 261: vacuum_in_place）和 repl.rs（line 5: GraphEngine）直接引用 nervusdb-storage，绕过 Db 门面层，导致同一数据库文件被打开两次，破坏单写者保证 |
| P2 | crates.io 包名碎片化 | 7 个 nervusdb 相关包（5 个 v2 + 2 个 v1 遗留），用户不知道该 `cargo add` 哪个；目录名和代码引用中残留 `-v2` 后缀；nervusdb 的 re-export 不完整（缺 `GraphStore` trait（lib.rs:50 只有 `use` 没有 `pub use`）、vacuum 模块、backup 模块、bulkload 模块、storage 层常量（PAGE_SIZE 等）） |
| P2 | TCK 文件名含数字前缀 | 测试文件使用 `tXXX_` 数字前缀命名（如 t155_edge_persistence, t306_unwind, t62_order_by_skip_test），TCK 100% 后统一重构为语义化命名 |

---

# 第二部分：优化架构方案

## 10. 重构总览

```
优化目标:
  1. 可维护性: 拆分巨型文件，引入 Plan 层
  2. 正确性: 统一类型定义，支持多重边
  3. 性能: Buffer Pool, 标签索引, 快照隔离改进
  4. 扩展性: VFS 抽象, 段合并, 容量扩展
```

优化后的架构分层：

```
┌──────────────────────────────────────────────────────┐
│  Language Bindings (Python / Node.js / WASM)         │
├──────────────────────────────────────────────────────┤
│  nervusdb (Facade)                                   │
│  Db │ ReadTxn │ WriteTxn │ DbSnapshot │ QueryExt     │
├──────────────────────────────────────────────────────┤
│  nervusdb-query (查询引擎 - 重构)                     │
│  Lexer → Parser → AST → Planner → Optimizer          │
│                           → PhysicalPlan → Executor  │
├──────────────────────────────────────────────────────┤
│  nervusdb-storage (存储引擎 - 增强)                   │
│  BufferPool │ WAL │ MemTable │ L0Run │ CSR           │
│  LabelIndex │ Compaction │ IdMap │ Index             │
├──────────────────────────────────────────────────────┤
│  nervusdb-api (统一类型 + Trait)                      │
│  PropertyValue │ EdgeKey │ GraphSnapshot │ GraphStore │
├──────────────────────────────────────────────────────┤
│  VFS 抽象层 (新增)                                    │
│  DefaultVFS │ MemoryVFS │ (EncryptedVFS)             │
└──────────────────────────────────────────────────────┘
```

---

## 11. 查询引擎重构

### 11.1 引入 Plan 层

当前直接从 AST 到执行，缺少中间表示。引入逻辑计划和物理计划：

```rust
// nervusdb-query/src/plan/logical.rs
enum LogicalPlan {
    // 扫描
    NodeScan { label: Option<String>, variable: String },
    IndexScan { label: String, property: String, value: Value, variable: String },

    // 图操作
    Expand {
        input: Box<LogicalPlan>,
        src_var: String,
        dst_var: String,
        edge_var: Option<String>,
        rel_types: Vec<String>,
        direction: Direction,
        var_length: Option<(u32, u32)>,
    },

    // 关系代数
    Filter { input: Box<LogicalPlan>, predicate: Expression },
    Project { input: Box<LogicalPlan>, expressions: Vec<(Expression, String)> },
    Sort { input: Box<LogicalPlan>, keys: Vec<(Expression, SortOrder)> },
    Limit { input: Box<LogicalPlan>, count: u64 },
    Skip { input: Box<LogicalPlan>, count: u64 },
    Distinct { input: Box<LogicalPlan> },
    Aggregate { input: Box<LogicalPlan>, group_keys: Vec<Expression>, aggregates: Vec<AggExpr> },

    // 写操作
    CreateNode { labels: Vec<String>, properties: Vec<(String, Expression)> },
    CreateEdge { src: String, dst: String, rel_type: String, properties: Vec<(String, Expression)> },
    DeleteNode { variable: String, detach: bool },
    SetProperty { variable: String, key: String, value: Expression },

    // 组合
    CartesianProduct { left: Box<LogicalPlan>, right: Box<LogicalPlan> },
    Optional { input: Box<LogicalPlan> },
    Union { left: Box<LogicalPlan>, right: Box<LogicalPlan>, all: bool },
}
```

### 11.2 查询优化器

```rust
// nervusdb-query/src/plan/optimizer.rs
struct RuleBasedOptimizer {
    rules: Vec<Box<dyn OptimizationRule>>,
}

trait OptimizationRule {
    fn apply(&self, plan: LogicalPlan, stats: &GraphStatistics) -> LogicalPlan;
}

// 内置规则
struct PredicatePushdown;     // WHERE 条件下推到 Expand 之前
struct IndexSelection;        // 有索引时 NodeScan → IndexScan
struct LabelIndexSelection;   // 有标签索引时使用 RoaringBitmap
struct ExpandDirectionChoice; // 选择度数低的方向展开
struct LimitPushdown;         // LIMIT 下推
struct RedundantFilterElim;   // 消除冗余过滤
```

### 11.3 executor.rs 拆分方案

```
当前: executor.rs (~242K chars, 单文件)

拆分为:
  executor/
  ├── mod.rs          # PhysicalOperator trait, Row, Value 定义
  ├── scan.rs         # NodeScanOp, IndexScanOp, LabelScanOp
  ├── expand.rs       # ExpandOp, VarLengthExpandOp
  ├── filter.rs       # FilterOp
  ├── project.rs      # ProjectOp
  ├── aggregate.rs    # AggregateOp (COUNT, SUM, AVG, MIN, MAX, COLLECT)
  ├── sort.rs         # SortOp (内存排序 + 外部排序)
  ├── limit.rs        # LimitOp, SkipOp
  ├── distinct.rs     # DistinctOp
  ├── mutate.rs       # CreateNodeOp, CreateEdgeOp, DeleteOp, SetOp, MergeOp
  ├── subquery.rs     # CallSubqueryOp, ExistsOp
  ├── union.rs        # UnionOp, UnionAllOp
  └── unwind.rs       # UnwindOp, ForeachOp
```

每个操作符实现统一的 trait：

```rust
trait PhysicalOperator {
    fn open(&mut self, ctx: &ExecutionContext) -> Result<()>;
    fn next(&mut self, ctx: &ExecutionContext) -> Result<Option<Row>>;
    fn close(&mut self) -> Result<()>;
}
```

### 11.4 evaluator.rs 拆分方案

```
当前: evaluator.rs (~166K chars, 单文件)

拆分为:
  evaluator/
  ├── mod.rs           # evaluate() 入口, Value 类型
  ├── arithmetic.rs    # +, -, *, /, %, ^
  ├── comparison.rs    # =, <>, <, >, <=, >=, IS NULL, IS NOT NULL
  ├── logical.rs       # AND, OR, NOT, XOR
  ├── string_ops.rs    # STARTS WITH, ENDS WITH, CONTAINS, toLower, toUpper, trim, ...
  ├── list_ops.rs      # IN, list comprehension, range, head, tail, ...
  ├── type_coercion.rs # toInteger, toFloat, toString, toBoolean
  ├── aggregate.rs     # count, sum, avg, min, max, collect
  └── functions.rs     # 内置函数 (id, labels, type, properties, keys, ...)
```

### 11.5 query_api.rs 拆分方案

```
当前: query_api.rs (~153K chars, 单文件)

拆分为:
  planner/
  ├── mod.rs              # prepare() 入口, PreparedQuery
  ├── match_planner.rs    # MATCH 子句 → LogicalPlan
  ├── write_planner.rs    # CREATE/DELETE/SET/MERGE → LogicalPlan
  ├── return_planner.rs   # RETURN/WITH → Project/Aggregate/Sort/Limit
  └── pattern_compiler.rs # Pattern → Expand 链
```

---

## 12. 存储引擎增强

### 12.1 Buffer Pool (页面缓存)

```rust
// nervusdb-storage/src/buffer_pool.rs
struct BufferPool {
    frames: Vec<BufferFrame>,
    page_table: HashMap<PageId, usize>,
    capacity: usize,
    clock_hand: usize,
    pager: Pager,  // 拥有 Pager，不再共享
}

struct BufferFrame {
    page_id: Option<PageId>,
    data: [u8; PAGE_SIZE],
    pin_count: AtomicU32,
    is_dirty: AtomicBool,
    reference_bit: AtomicBool,
}

impl BufferPool {
    fn fetch_page(&self, page_id: PageId) -> Result<PageGuard>;  // 自动 pin
    fn new_page(&mut self) -> Result<(PageId, PageGuard)>;
    fn flush_page(&mut self, page_id: PageId) -> Result<()>;
    fn flush_all_dirty(&mut self) -> Result<()>;
}
```

关键改进：
- 所有页面访问通过 BufferPool，不再直接操作 Pager
- Clock-Sweep 淘汰策略
- pin/unpin 机制防止活跃页面被淘汰
- 脏页追踪，checkpoint 时批量刷盘
- 默认 256 页 (2MB with 8KB pages)，可配置

### 12.2 VFS 抽象层

```rust
// nervusdb-storage/src/vfs/mod.rs
trait Vfs: Send + Sync {
    type File: VfsFile;
    fn open(&self, path: &Path, flags: OpenFlags) -> Result<Self::File>;
    fn delete(&self, path: &Path) -> Result<()>;
    fn exists(&self, path: &Path) -> bool;
}

trait VfsFile: Send + Sync {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: u64, data: &[u8]) -> Result<usize>;
    fn sync(&self) -> Result<()>;
    fn size(&self) -> Result<u64>;
    fn truncate(&self, size: u64) -> Result<()>;
}

// 内置实现
struct DefaultVfs;   // OS 文件系统
struct MemoryVfs;    // 纯内存（测试用）
```

### 12.3 标签索引 (RoaringBitmap)

```rust
// nervusdb-storage/src/label_index.rs
struct LabelIndex {
    label_to_nodes: HashMap<LabelId, RoaringBitmap>,
}

impl LabelIndex {
    fn add_node(&mut self, node: InternalNodeId, label: LabelId);
    fn remove_node(&mut self, node: InternalNodeId, label: LabelId);
    fn nodes_with_label(&self, label: LabelId) -> &RoaringBitmap;
    fn nodes_with_labels(&self, labels: &[LabelId]) -> RoaringBitmap;  // 交集
    fn count(&self, label: LabelId) -> u64;
}
```

性能提升：
- `MATCH (n:Person)` 从 O(N) 全扫描 → O(|Person|) 位图迭代
- 多标签过滤通过位图交集实现
- 内存开销小（RoaringBitmap 压缩）

### 12.4 CSR 段合并 (Level Compaction)

```rust
// nervusdb-storage/src/compaction.rs
struct CompactionPolicy {
    max_l0_runs: usize,        // L0Run 数量阈值，默认 4
    max_segments_per_level: usize,  // 每层最大段数，默认 4
    size_ratio: f64,           // 层间大小比，默认 10.0
}

impl CompactionPolicy {
    fn should_compact_l0(&self, runs: &[L0Run]) -> bool;
    fn should_merge_segments(&self, segments: &[CsrSegment]) -> Option<MergePlan>;
}

struct MergePlan {
    input_segments: Vec<SegmentId>,
    output_level: usize,
}
```

### 12.5 多页 Bitmap 扩展（Overflow Bitmap Pages）

#### 12.5.1 瓶颈分析

```
根因 (pager.rs):
  const BITMAP_BITS: u64 = (PAGE_SIZE as u64) * 8;  // = 65536
  struct Bitmap { data: [u8; PAGE_SIZE] }             // 固定 1 页

  1 bit = 1 page (8KB)
  65536 bits × 8KB = 512MB 硬上限

  allocate_page() 在 candidate >= BITMAP_BITS 时返回 Error::PageIdOutOfRange
  validate_data_page_id() 在 page_id >= BITMAP_BITS 时拒绝访问
```

#### 12.5.2 方案：Meta 内嵌 Bitmap 页数组

核心思路：Bitmap 从固定 1 页变为按需增长 N 页。额外 bitmap 页从数据区分配，其 PageId 记录在 Meta 页的空闲空间中。

```
改后的文件布局:

  Page 0:  Meta（新增 extra_bitmap_count + extra_bitmap_page_ids[]）
  Page 1:  Primary Bitmap（不变，管理 PageId 0..65535 = 前 512MB）
  Page 2+: Data Pages（不变）
    ...
  Page X:  Overflow Bitmap 1（从数据页分配，管理 PageId 65536..131071）
  Page Y:  Overflow Bitmap 2（管理 PageId 131072..196607）
    ...

  关键: overflow bitmap 页是普通数据页，由 Meta 直接追踪，不走 bitmap 自身分配。
```

容量计算：

```
Meta 页 = 8192 bytes，当前使用 offset 0..91 (92 bytes)，剩余 8100 bytes
每个 overflow page id = 8 bytes → 最多 (8100 - 8) / 8 ≈ 1011 个 overflow 页
总 bitmap 页 = 1 (primary) + 1011 (overflow) = 1012
总容量 = 1012 × 65536 × 8KB ≈ 506GB
```

#### 12.5.3 Meta 扩展

```rust
struct Meta {
    // --- 现有字段 (offset 0..91) ---
    version_major: u32,            // [16..20]
    version_minor: u32,            // [20..24]
    page_size: u64,                // [24..32]
    bitmap_page_id: u64,           // [32..40]
    next_page_id: u64,             // [40..48]
    i2e_start_page_id: u64,        // [48..56]
    i2e_len: u64,                  // [56..64]
    next_internal_id: u64,         // [64..72]
    index_catalog_root: u64,       // [72..80]
    next_index_id: u32,            // [80..84]
    storage_format_epoch: u64,     // [84..92]

    // --- 新增字段 ---
    extra_bitmap_count: u64,       // [92..100]  默认 0 = 传统单页模式
    extra_bitmap_page_ids: Vec<u64>, // [100..100+N*8]  overflow bitmap 页的 PageId
}
```

向后兼容：旧文件 offset 92..99 全为零 → `extra_bitmap_count = 0` → 单页 bitmap，行为不变。

#### 12.5.4 Bitmap 结构改造

```rust
// 当前
struct Bitmap {
    data: [u8; PAGE_SIZE],  // 固定 1 页
}

// 改为
struct Bitmap {
    pages: Vec<[u8; PAGE_SIZE]>,  // pages[0] = Primary (Page 1)
}                                  // pages[1..] = Overflow

impl Bitmap {
    fn get_bit(&self, bit: u64) -> bool {
        let bits_per_page = (PAGE_SIZE * 8) as u64;  // 65536
        let page_idx = (bit / bits_per_page) as usize;
        let bit_in_page = bit % bits_per_page;
        let byte_idx = (bit_in_page / 8) as usize;
        let mask = 1u8 << (bit_in_page % 8);
        self.pages.get(page_idx)
            .map(|p| (p[byte_idx] & mask) != 0)
            .unwrap_or(false)  // 超出范围视为未分配
    }

    fn set_bit(&mut self, bit: u64, value: bool) {
        let bits_per_page = (PAGE_SIZE * 8) as u64;
        let page_idx = (bit / bits_per_page) as usize;
        let bit_in_page = bit % bits_per_page;
        let byte_idx = (bit_in_page / 8) as usize;
        let mask = 1u8 << (bit_in_page % 8);
        if let Some(page) = self.pages.get_mut(page_idx) {
            if value { page[byte_idx] |= mask; }
            else     { page[byte_idx] &= !mask; }
        }
    }

    fn find_free_in_range(&self, start: u64, end: u64) -> Option<u64> {
        (start..end).find(|&id| !self.get_bit(id))
    }
}
```

`BITMAP_BITS` 常量改为 Pager 方法：

```rust
// 删除: const BITMAP_BITS: u64 = (PAGE_SIZE as u64) * 8;

impl Pager {
    fn bitmap_capacity(&self) -> u64 {
        self.bitmap.pages.len() as u64 * (PAGE_SIZE as u64) * 8
    }
}
```

#### 12.5.5 allocate_page() 自动扩展

```rust
pub fn allocate_page(&mut self) -> Result<PageId> {
    loop {
        let candidate = self.bitmap
            .find_free_in_range(FIRST_DATA_PAGE_ID.as_u64(), self.meta.next_page_id)
            .unwrap_or(self.meta.next_page_id);

        if candidate >= self.bitmap_capacity() {
            self.grow_bitmap()?;  // 自动扩展，然后重新搜索
            continue;             // grow_bitmap 会占用一个页面，需要重新找空闲位
        }

        if candidate == self.meta.next_page_id {
            self.meta.next_page_id = candidate + 1;
        }

        self.ensure_allocated(PageId::new(candidate))?;
        return Ok(PageId::new(candidate));
    }
}
```

#### 12.5.6 grow_bitmap() 实现

```rust
fn grow_bitmap(&mut self) -> Result<()> {
    let max_extra = (PAGE_SIZE - 100) / 8;  // ~1011
    if self.bitmap.pages.len() - 1 >= max_extra {
        return Err(Error::MaxBitmapCapacity);  // ~506GB，几乎不可能触发
    }

    // 直接用 next_page_id 分配（绕过 bitmap 查找，解决鸡生蛋问题）
    let new_page_id = self.meta.next_page_id;
    self.meta.next_page_id = new_page_id + 1;

    // 扩展文件
    let required = (new_page_id + 1) * PAGE_SIZE as u64;
    if self.file.metadata()?.len() < required {
        self.file.set_len(required)?;
    }

    // 初始化新 bitmap 页并添加到内存
    let new_bitmap_page = [0u8; PAGE_SIZE];
    self.bitmap.pages.push(new_bitmap_page);

    // 在 bitmap 中标记此页为已分配（防止被 allocate_page 覆盖）
    // 注意: 此时 pages 已包含新页，set_bit 可以正确寻址
    self.bitmap.set_bit(new_page_id, true);

    // 记录到 Meta
    self.meta.extra_bitmap_page_ids.push(new_page_id);
    self.meta.extra_bitmap_count = self.meta.extra_bitmap_page_ids.len() as u64;

    self.flush_meta_and_bitmap()
}
```

> **崩溃安全性注意**：`grow_bitmap()` 在 `set_len` 扩展文件和 `flush_meta_and_bitmap()` 之间崩溃时，
> 文件已扩展但 Meta 未记录新 bitmap 页。建议在 `open()` 时检测 `file.len() > next_page_id * PAGE_SIZE`
> 的不一致状态并修复（截断多余空间或重新初始化）。

> **鸡生蛋问题**：bitmap 满了才需要 grow，但 grow 需要分配新页面。
> 解决：新 bitmap 页直接用 `next_page_id`（文件末尾追加），不走 bitmap 查找。
> 新页面由 Meta 的 `extra_bitmap_page_ids` 数组管理，不在 bitmap 中作为空闲页追踪。
> 但在 bitmap 中标记为 allocated，防止后续 `allocate_page()` 将其当作数据页分配。

#### 12.5.7 其他需要适配的方法

```rust
// validate_data_page_id(): 拒绝 bitmap 页
fn validate_data_page_id(&self, page_id: PageId) -> Result<()> {
    let id = page_id.as_u64();
    if id < FIRST_DATA_PAGE_ID.as_u64()
        || id >= self.bitmap_capacity()
        || self.meta.extra_bitmap_page_ids.contains(&id)
    {
        return Err(Error::PageIdOutOfRange(id));
    }
    Ok(())
}

// flush_meta_and_bitmap(): 刷新所有 bitmap 页
fn flush_meta_and_bitmap(&mut self) -> Result<()> {
    write_page_raw(&self.file, META_PAGE_ID, &self.meta.encode_page())?;
    write_page_raw(&self.file, BITMAP_PAGE_ID, &self.bitmap.pages[0])?;
    for (i, &page_id) in self.meta.extra_bitmap_page_ids.iter().enumerate() {
        write_page_raw(&self.file, PageId::new(page_id), &self.bitmap.pages[i + 1])?;
    }
    self.file.sync_data()
}

// open(): 加载所有 bitmap 页
fn open(path: impl AsRef<Path>) -> Result<Self> {
    // ... 现有逻辑 ...
    let mut bitmap_pages = vec![bitmap_page];
    for &extra_id in &meta.extra_bitmap_page_ids {
        let mut page = [0u8; PAGE_SIZE];
        read_page_raw(&file, PageId::new(extra_id), &mut page)?;
        bitmap_pages.push(page);
    }
    let bitmap = Bitmap { pages: bitmap_pages };
    // ...
}

// write_vacuum_copy(): 适配多 bitmap 页
// - reachable 集合的 page_id 上限从 BITMAP_BITS 改为 bitmap_capacity()
// - 拷贝时需要写入所有 bitmap 页
```

#### 12.5.8 向后兼容性

| 场景 | 行为 |
|------|------|
| 新版本读旧文件 | `extra_bitmap_count=0` → 单页 bitmap，完全兼容 |
| 旧版本读新文件（数据 < 512MB） | 忽略未知 Meta 字段，正常工作 |
| 旧版本读新文件（数据 > 512MB） | 无法访问 overflow 区域数据，不损坏文件 |

不需要 bump `storage_format_epoch`。旧版本的 `validate_data_page_id` 会拒绝 >= 65536 的 page id，不会误写 overflow 区域。

#### 12.5.9 容量对比

| | 当前 | 改后 |
|--|------|------|
| Bitmap 页数 | 1（固定） | 1 ~ 1012（按需增长） |
| 最大容量 | **512MB** | **~506GB** |
| 空间开销 | 0 | 每增 512MB 多 1 个 8KB 页 |
| 性能影响 | 无 | bitmap 查找多一次数组索引（O(1)） |
| 改动范围 | — | 仅 pager.rs，约 100 行 |

#### 12.5.10 改动清单

| # | 改动 | 类型 |
|---|------|------|
| 1 | `Meta` 新增 `extra_bitmap_count` + `extra_bitmap_page_ids` | 结构体修改 |
| 2 | `Meta::encode_page()` / `decode_page()` 编解码新字段 | 方法修改 |
| 3 | `Bitmap` 从 `[u8; PAGE_SIZE]` 改为 `Vec<[u8; PAGE_SIZE]>` | 结构体修改 |
| 4 | `Bitmap::get_bit()` / `set_bit()` / `find_free_in_range()` 多页寻址 | 方法修改 |
| 5 | `BITMAP_BITS` 常量删除，改为 `Pager::bitmap_capacity()` 方法 | 常量→方法 |
| 6 | `allocate_page()` 增加 `grow_bitmap()` 循环 | 方法修改 |
| 7 | `grow_bitmap()` 新方法 | 新增 |
| 8 | `validate_data_page_id()` 适配动态上限 + 排除 overflow bitmap 页 | 方法修改 |
| 9 | `flush_meta_and_bitmap()` 刷新所有 bitmap 页 | 方法修改 |
| 10 | `open()` 加载 overflow bitmap 页 | 方法修改 |
| 11 | `write_vacuum_copy()` 适配多 bitmap 页 | 方法修改 |

---

## 13. 并发模型改进

### 13.1 消除 Pager 锁竞争

```
当前:
  StorageSnapshot 持有 Arc<RwLock<Pager>>
  → 读属性需要 Pager 读锁
  → compact 需要 Pager 写锁
  → 读写互相阻塞

改进:
  StorageSnapshot 通过 BufferPool 读取
  → BufferPool 内部使用细粒度 latch（每个 frame 独立）
  → 读操作不阻塞写操作
  → 写操作通过 COW 不影响旧快照
```

### 13.2 无锁快照创建

```
当前:
  snapshot() → 锁 idmap → scan_i2e_records() → O(N) 拷贝

改进:
  写事务提交时更新 published_i2e: RwLock<Arc<Vec<I2eRecord>>>
  snapshot() → clone Arc → O(1)
```

### 13.3 锁粒度优化

```
当前                          改进
idmap: Mutex                → RwLock（读多写少）
label_interner: Mutex       → RwLock
index_catalog: Mutex        → RwLock
```

---

## 14. 数据模型增强

### 14.1 统一类型定义

消除 PropertyValue 和 EdgeKey 在 api/storage 层的重复定义（~100 行转换代码）：

```
当前:
  nervusdb-api::PropertyValue     (公共)
  nervusdb-storage::PropertyValue  (存储层，几乎相同)
  nervusdb-query::Value            (查询运行时，必须保留，含 NodeId/EdgeKey 等运行时变体)
  + convert_property_value() / to_storage() 转换函数 (api.rs:444-480)

改进:
  nervusdb-api::PropertyValue  ← 唯一定义（api + storage 统一）
  nervusdb-query::Value        ← 保留（查询运行时需要额外变体）
  消除 api ↔ storage 层的转换代码（~100 行）
```

注意：query 层的 `Value` enum 必须保留，因为它包含 `NodeId`、`ExternalId`、`EdgeKey` 等查询运行时特有的变体，不适合下沉到 api 层。

同样统一 EdgeKey：

```
当前:
  nervusdb-api::EdgeKey
  nervusdb-storage::snapshot::EdgeKey  (重复定义)
  + conv() 转换函数

改进:
  nervusdb-api::EdgeKey ← 唯一定义
```

### 14.2 多重边支持

```rust
// 引入全局唯一 EdgeId
pub type EdgeId = u64;

pub struct EdgeKey {
    pub id: EdgeId,              // 新增：全局唯一边 ID
    pub src: InternalNodeId,
    pub rel: RelTypeId,
    pub dst: InternalNodeId,
}
```

影响：
- MemTable: 边去重改为按 EdgeId
- CsrSegment: EdgeRecord 增加 id 字段（磁盘格式不兼容）
- WAL: CreateEdge 增加 edge_id（WAL 格式迁移）
- 属性存储: 边属性键从 (src, rel, dst) 改为 EdgeId（B-Tree 键编码变更）
- 绑定 API: Python/Node.js 绑定需要适配新的 EdgeKey
- 数据迁移: 需要提供旧格式 → 新格式的迁移工具

> 由于涉及 WAL 格式、CSR 磁盘格式、B-Tree 键编码等多处不兼容变更，建议从 Phase 3 移至 Phase 4，在其他基础设施就绪后再实施。

### 14.3 属性键字符串字典

```rust
// nervusdb-storage/src/property_key_interner.rs
struct PropertyKeyInterner {
    key_to_id: HashMap<String, u32>,
    id_to_key: Vec<String>,
}
```

将属性键 `String` 映射为 `u32`，减少内存和存储开销。

---

## 15. 索引增强

### 15.1 索引回填

```rust
impl GraphEngine {
    /// 创建索引并回填现有数据
    pub fn create_index_with_backfill(&self, label: &str, property: &str) -> Result<()> {
        // 1. 创建索引定义
        // 2. 扫描所有匹配 label 的节点
        // 3. 对每个节点，读取 property 值
        // 4. 插入索引
        // 5. 标记索引为 Online
    }
}
```

### 15.2 复合索引

```rust
struct IndexDef {
    name: String,
    label: String,
    properties: Vec<String>,  // 支持多属性复合索引
    root: PageId,
    state: IndexState,  // Building / Online / Offline
}
```

---

## 16. 重构优先级与路线图

> 通用门禁：每个 PR 必须通过 `fmt + clippy + workspace_quick_test + tier0/1/2 TCK + bindings smoke + contract smoke`。
> 回滚条件：任一门禁失败立即回滚 PR，不允许带红合入。

### Phase 0: 审计与护栏（前置）

| 任务 | 说明 | 影响 |
|------|------|------|
| 冻结事实基线 | 记录当前 TCK 通过率、NotImplemented 计数、文件大小等指标快照 | 重构前后可量化对比 |
| 建立回归集 | 锁定 feature tests + tier0/1/2 TCK 作为拆分回归集 | 确保行为等价 |
| 统一证据口径 | 关键指标从 artifacts 自动读取，不允许手填 | 文档不再漂移 |

### Phase 1a: 纯文件拆分 + CLI 边界收敛（严格行为等价，可与 Beta 推进并行）

> 范围约束：本阶段只做"移动代码"和"收敛调用路径"，不改类型定义、不改包名、不改公共 API 签名。

| 任务 | 说明 | 影响 |
|------|------|------|
| 拆分 executor.rs | ~242K → 12 个文件（行为等价，不改语义） | 可维护性大幅提升 |
| 拆分 evaluator.rs | ~166K → 8 个文件 | 可维护性大幅提升 |
| 拆分 query_api.rs | ~153K → 4 个文件 | 可维护性大幅提升 |
| 修复 CLI 依赖 | CLI 只依赖 nervusdb 主包，不直接引用 nervusdb-storage 子包（main.rs + repl.rs） | 消除双开数据库文件的设计缺陷 |

### Phase 1b: 类型统一 + 包名收敛（语义变更，需独立验证）

> 范围约束：本阶段涉及公共 API 变更和 crate 发布，必须独立于文件拆分单独提交和验证。

| 任务 | 说明 | 影响 |
|------|------|------|
| 统一 PropertyValue/EdgeKey | 消除 api/storage 层重复定义和转换代码 | 减少 ~100 行转换代码 |
| 包名收敛 | 去掉 -v2 后缀（目录名 + Cargo.toml + 代码中的 `nervusdb_*` 引用），抢注 nervusdb 包名，补全 re-export（`GraphStore` trait（lib.rs:50 只有 `use` 没有 `pub use`）、vacuum 模块、backup 模块、bulkload 模块、storage 层常量（PAGE_SIZE 等）） | 用户只需 `cargo add nervusdb` |
| TCK 文件名清理 | TCK 通过率达到 100% 后，清理测试文件名中的 `tXXX_` 数字前缀（如 t155_edge_persistence, t306_unwind），统一重构为语义化命名 | 代码整洁度 |

### Phase 1c: 引入 LogicalPlan（需在 Phase 1a 后进行，改变查询管线）

| 任务 | 说明 | 影响 |
|------|------|------|
| 引入 LogicalPlan | AST → LogicalPlan → Executor | 为优化器做准备，改变查询管线 |

### Phase 2: 性能

| 任务 | 说明 | 影响 |
|------|------|------|
| Buffer Pool | 页面缓存，Clock-Sweep（需预留 VFS trait 接口，或将 VFS 提前到 Phase 2） | 读性能提升 10x+ |
| 标签索引 | RoaringBitmap | 标签查询从 O(N) → O(K) |
| 快照隔离改进 | 消除 Pager 锁竞争 | 读写不阻塞 |
| 索引回填 | 创建索引时回填旧数据 | 索引可用性 |
| 查询优化器 | 谓词下推、索引选择 | 查询性能提升 |

### Phase 3: 扩展性（v1.0 前）

| 任务 | 说明 | 影响 |
|------|------|------|
| VFS 抽象层 | 文件 I/O 抽象 | 可测试性、加密支持 |
| CSR 段合并 | Level Compaction | 读性能稳定 |
| 多页 Bitmap | Overflow Bitmap Pages（详见 12.5） | 512MB → ~506GB |
| 属性键字典 | 字符串池化 | 内存节省 30%+ |

### Phase 4: 生产就绪

| 任务 | 说明 | 影响 |
|------|------|------|
| 多重边 (EdgeId) | 全局唯一边 ID（涉及 WAL/CSR/B-Tree 格式迁移） | 图语义完整 |
| 页面校验和 | CRC32C per page | 数据完整性 |
| 压缩 (LZ4) | 页面级透明压缩 | 存储节省 50%+ |
| CBO 优化器 | 基于代价的查询优化 | 复杂查询性能 |
| WASM 支持 | wasm32 编译目标 | 浏览器/边缘 |

---

## 17. 重构后的项目结构

```
nervusdb/
├── crates/
│   ├── nervusdb-api/                  # 统一类型 + Trait         → crates.io: nervusdb-api (内部)
│   │   └── src/lib.rs                 # PropertyValue, EdgeKey, GraphSnapshot, ...
│   │
│   ├── nervusdb-storage/              # 存储引擎                 → crates.io: nervusdb-storage (内部)
│   │   └── src/
│   │       ├── vfs/                   # [新增] VFS 抽象
│   │       │   ├── mod.rs
│   │       │   ├── default.rs
│   │       │   └── memory.rs
│   │       ├── buffer_pool.rs         # [新增] 页面缓存
│   │       ├── pager.rs               # 页面管理（被 BufferPool 包装）
│   │       ├── wal.rs                 # WAL
│   │       ├── memtable.rs            # 写缓冲
│   │       ├── snapshot.rs            # L0Run + 快照
│   │       ├── csr.rs                 # CSR 段
│   │       ├── compaction.rs          # [新增] 段合并策略
│   │       ├── idmap.rs               # ID 映射
│   │       ├── label_interner.rs      # 标签字典
│   │       ├── label_index.rs         # [新增] 标签索引 (RoaringBitmap)
│   │       ├── property.rs            # 属性编码
│   │       ├── property_key_interner.rs # [新增] 属性键字典
│   │       ├── engine.rs              # 引擎核心
│   │       ├── blob_store.rs
│   │       ├── backup.rs
│   │       ├── bulkload.rs
│   │       ├── vacuum.rs
│   │       ├── stats.rs
│   │       └── index/
│   │           ├── btree.rs
│   │           ├── catalog.rs
│   │           ├── hnsw/
│   │           └── ordered_key.rs
│   │
│   ├── nervusdb-query/                # 查询引擎（重构）          → crates.io: nervusdb-query (内部)
│   │   └── src/
│   │       ├── lexer.rs
│   │       ├── parser.rs
│   │       ├── ast.rs
│   │       ├── plan/                  # [新增] 计划层
│   │       │   ├── mod.rs
│   │       │   ├── logical.rs         # LogicalPlan
│   │       │   ├── physical.rs        # PhysicalPlan
│   │       │   └── optimizer.rs       # 规则优化器
│   │       ├── planner/               # [重构] 从 query_api.rs 拆出
│   │       │   ├── mod.rs
│   │       │   ├── match_planner.rs
│   │       │   ├── write_planner.rs
│   │       │   ├── return_planner.rs
│   │       │   └── pattern_compiler.rs
│   │       ├── executor/              # [重构] 从 executor.rs 拆出
│   │       │   ├── mod.rs
│   │       │   ├── scan.rs
│   │       │   ├── expand.rs
│   │       │   ├── filter.rs
│   │       │   ├── project.rs
│   │       │   ├── aggregate.rs
│   │       │   ├── sort.rs
│   │       │   ├── limit.rs
│   │       │   ├── distinct.rs
│   │       │   ├── mutate.rs
│   │       │   ├── subquery.rs
│   │       │   ├── union.rs
│   │       │   └── unwind.rs
│   │       ├── evaluator/             # [重构] 从 evaluator.rs 拆出
│   │       │   ├── mod.rs
│   │       │   ├── arithmetic.rs
│   │       │   ├── comparison.rs
│   │       │   ├── logical.rs
│   │       │   ├── string_ops.rs
│   │       │   ├── list_ops.rs
│   │       │   ├── type_coercion.rs
│   │       │   ├── aggregate.rs
│   │       │   └── functions.rs
│   │       └── facade.rs
│   │
│   ├── nervusdb/                      # 门面层                   → crates.io: nervusdb (对外)
│   ├── nervusdb-cli/                  #                          → crates.io: nervusdb-cli (对外)
│   ├── nervusdb-pyo3/                 #                          → PyPI: nervusdb (Rust 稳定后发布)
│   └── nervusdb-node/                 #                          → npm: nervusdb (Rust 稳定后发布)
│
├── tests/                             # 集成测试（tXXX_ 文件，后续重构为语义化命名）
├── fuzz/                              # 模糊测试（独立 workspace，依赖 nervusdb-query）
└── scripts/                           # TCK 门控、基准测试、发布等脚本
```

---

## 18. 关键设计决策对照

| # | 维度 | 现状 | 优化方案 | 权衡 |
|---|------|------|----------|------|
| 1 | 页面访问 | 直接 Pager I/O | Buffer Pool + Clock-Sweep | 读性能 ✅ / 内存开销 ⚠️ |
| 2 | 查询处理 | AST → 直接执行 | AST → LogicalPlan → Optimizer → PhysicalPlan → Executor | 可优化 ✅ / 查询管线复杂度 ⚠️ |
| 3 | 代码组织 | 3 个巨型文件 (~561K chars) | 24+ 个模块化文件 | 可维护性 ✅ / 拆分迁移成本 ⚠️ |
| 4 | 类型定义 | PropertyValue/EdgeKey 重复 | 统一在 nervusdb-api | 代码简洁 ✅ / api crate 依赖增加 ⚠️ |
| 5 | 标签查询 | 全扫描 O(N) | RoaringBitmap O(K) | 查询性能 ✅ / 写入时维护位图开销 ⚠️ |
| 6 | 快照隔离 | Pager RwLock 阻塞 | BufferPool + COW 无阻塞 | 并发性能 ✅ / 实现复杂度 ⚠️ |
| 7 | 边标识 | (src, rel, dst) 无多重边 | EdgeId 支持多重边 | 图语义完整 ✅ / WAL/CSR/B-Tree 格式迁移成本 ⚠️ |
| 8 | 文件 I/O | 直接 OS 调用 | VFS 抽象层 | 可测试性 ✅ / 间接调用开销 ⚠️ |
| 9 | 段管理 | 只追加，不合并 | Level Compaction | 读性能稳定 ✅ / 后台 I/O 开销 ⚠️ |
| 10 | 容量 | 512MB (单 Bitmap 页) | Overflow Bitmap Pages，~506GB（详见 12.5） | 容量扩展 ✅ / Meta 页空间占用 ⚠️ |
| 11 | 发布策略 | 7 个包，用户困惑 | 2 个对外包 (nervusdb + nervusdb-cli)，内部子包标记为实现细节 | 用户体验 ✅ / 迁移成本 ⚠️ |

---

> **重构原则**：渐进式重构，先结构等价再语义变更，每个 Phase 独立可交付，不破坏现有测试。
> Phase 0 冻结基线并建立回归集。Phase 1a 纯文件拆分 + CLI 收敛，严格行为等价。
> Phase 1b 类型统一 + 包名收敛，涉及语义变更，需独立验证。Phase 1c 引入 LogicalPlan，改变查询管线。
> 每个阶段必须附：目标、前置条件、门禁、回滚条件。
