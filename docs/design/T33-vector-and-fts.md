# T33: Vector Index + Full-Text Search (FTS)

## 1. Context

NervusDB 已具备接近完整的 Cypher 读写能力与可靠持久化（redb + crash gate）。下一步要服务 GraphRAG / 本地检索增强场景，必须补齐两块“检索基础设施”：

- **向量相似检索**（embedding / HNSW）
- **全文检索**（BM25 / Lucene-like）

约束条件来自嵌入式定位：

- **用户 API 只传一个 path**（例如 `mydb.redb`），不引入外部服务。
- **Source of Truth 必须在 redb 内**：Sidecar 只做加速；丢了也能通过扫描 redb 重建。
- **依赖必须可控且 feature-gated**：默认关闭，锁定版本号，不引入 git/unstable 依赖。
- **一致性策略**：Vector 强一致（内存索引与事务一致），FTS 最终一致（NRT），允许显式 flush。

## 2. Goals

### 2.1 MVP Goals

1. 引入 `vector` 与 `fts` Cargo features（默认关闭）。
2. Raw Data（vector/text）仍以**节点/边属性**持久化在 `redb`（FlexBuffers）。
3. 允许受控 Sidecar：
   - `mydb.redb.usearch`：向量索引快照
   - `mydb.redb.tantivy/`：FTS 索引目录
4. `Database::open`：
   - 检测 Sidecar，能加载则加载
   - Sidecar 缺失/损坏/版本不兼容 → 触发重建（全量扫描 redb）
5. 查询扩展（函数式，侵入最小）：
   - `vec_similarity(n.prop, $query_vec)` → `float`
   - `txt_score(n.prop, $query_string)` → `float`
6. 索引生命周期：
   - Vector：写事务内同步更新内存索引；commit 后标记 dirty，并在 `flush_indexes()` / `close` 时落盘到 `.usearch`
   - FTS：写入先进入 IndexWriter buffer；`flush_indexes()` 提交 tantivy commit（允许 NRT 延迟）

### 2.2 Non-Goals (MVP 不做)

- 不改 Cypher `MATCH` 语法（例如 `NEAR`/`~` 这种语法糖）。
- 不做 `CALL PROCEDURE / YIELD` 与插件体系。
- 不把 tantivy/usearch 的索引强行塞进 `.redb` 文件内部（先用 Sidecar，后续再评估）。
- 不承诺 Neo4j 级别的全文语言特性（分词/同义词/多字段投影等先按最小集）。

## 3. Cargo Features / Dependencies

### 3.1 `nervusdb-core/Cargo.toml`

新增可选依赖（锁定版本号）：

- `usearch = "2.22.0"`（仅在 `cfg(not(target_arch = "wasm32"))` 下启用，且 `optional = true`）
- `tantivy = "0.25.0"`（仅在 `cfg(not(target_arch = "wasm32"))` 下启用，且 `optional = true`）
- `half = "2.7.1"`（`vector` 子依赖，`optional = true`）

新增 features（默认不启用）：

- `vector = ["dep:usearch", "dep:half"]`
- `fts = ["dep:tantivy"]`

### 3.2 其他 crate（绑定层）

绑定 crate（Node/Python）不强制开启 vector/fts；由上层选择是否打开对应 feature（类似现有 `temporal`）。

## 4. Storage Layout

### 4.1 Raw Data（redb 内）

**原则：不新增“第二份真相”。** Raw Data 仍存于现有属性表：

- `node_props_v2`（`TABLE_NODE_PROPS_BINARY`）
- `edge_props_v2`（`TABLE_EDGE_PROPS_BINARY`）

属性编码使用现有 FlexBuffers（`storage/property.rs`）。Vector 与 Text 均作为属性值的一部分保存。

### 4.2 Vector 的 FlexBuffers 表示（规范）

在 FlexBuffers 中使用“数值数组”表示向量：

- 语义：`Vec<f32>`（落地时允许 `f64` 输入，读取时转换为 `f32`）
- 约束：
  - 维度必须固定（由索引配置决定）
  - 非数值元素/维度不匹配 → 视为无向量（索引不收录；查询函数返回 `Null`）

> 注：MVP 不引入新的 property 二进制格式前缀，避免破坏现有属性兼容性；必要时可在 vNext 用专用 magic + typed blob 提速。

### 4.3 Sidecar 命名与加载

以 `redb_path = <Options.path>.with_extension("redb")` 为基准：

- Vector：`<redb_path>.usearch`（即 `mydb.redb.usearch`）
- FTS：`<redb_path>.tantivy/`（即 `mydb.redb.tantivy/`）

### 4.4 Sidecar 版本/兼容性

需要显式版本护栏，避免 silent corruption：

1. 在 `TABLE_META` 写入：
   - `vector.index.version`
   - `vector.index.config`（JSON：metric/dim/label/property/…）
   - `fts.index.version`
   - `fts.index.config`（JSON：schema/fields/label/property/…）
2. Sidecar 内也保存一份同样的 `meta.json`（或 usearch/tantivy 自带元数据 + 我们的额外校验）。

`Database::open` 时进行三层校验：

- meta key 存在且可解析
- Sidecar 存在且可打开
- Sidecar meta 与 redb meta 完全一致

任何失败 → **降级为重建**（scan redb → rebuild sidecar）。

## 5. APIs / Executor Changes

### 5.1 `query::executor::Value` 扩展

新增：

- `Value::Vector(Vec<f32>)`

并在以下路径补齐解析：

- 参数绑定：`serde_value_to_executor_value` 支持 JSON 数组（number list）→ `Vector`
- 属性访问：`PropertyAccess` 解析到 JSON array(number) → `Vector`

### 5.2 Cypher 函数

#### 5.2.1 `vec_similarity(a, b) -> float`

- 输入：`Vector`/可解析为 `Vector` 的值
- 输出：默认 **cosine similarity**
- 失败：维度不匹配/无法解析 → `Null`

#### 5.2.2 `txt_score(n.prop, query) -> float`

为避免“每行都跑一次索引查询”的灾难，必须做 per-query cache：

- 对同一个 `(label?, property, query_string)`，只向 tantivy 发起一次搜索，拿到 topN 结果并建立 `node_id -> score` map。
- 行级求值时 O(1) 查 map：
  - 命中 → 返回 score
  - 未命中 → 返回 0（或 `Null`，MVP 需要固定语义）

> 约束：函数参数必须是 `PropertyAccess`（例如 `txt_score(n.content, $q)`），否则无法定位到 node_id，返回 `Null`。

### 5.3 新增索引生命周期 API

（名称待实现阶段确认，避免破坏现有 API）

- `Database::flush_indexes() -> Result<()>`
  - Vector：将内存索引快照写入 `.usearch`
  - FTS：commit tantivy IndexWriter（落盘）
- `Database::rebuild_indexes() -> Result<()>`（可选）
  - 强制从 redb 扫描重建 sidecar（用于 sidecar 丢失/损坏）

## 6. Consistency Model

### 6.1 Vector：强一致（事务内一致）

目标：写事务内对向量的变更能同步反映到内存索引，并能回滚。

实现策略（MVP 选一个最简单可行的）：

1. **Undo Log**（优先）：
   - 事务内记录每次变更的 `(node_id, old_vector, new_vector)`
   - commit：清空 log，标记 dirty
   - abort：按 log 逆序恢复 old_vector
2. 若底层库不支持 delete/update 的可靠回滚，则退化为：
   - committed base index + per-tx overlay（小索引），查询时 merge（复杂度更高，作为备选方案）

### 6.2 FTS：最终一致（NRT）

- 属性写入只保证 redb 持久化。
- FTS 索引允许落后；在 `flush_indexes()` 或后台 commit 后可见。
- 事务回滚：
  - 建议将“写入 tantivy writer”延后到 `commit_transaction()` 之后，避免 rollback 清理成本。

## 7. Failure Modes / Fallback

必须明确处理以下失败模式：

1. **Sidecar 缺失**：open 时重建；服务可用但冷启动变慢。
2. **Sidecar 损坏**：校验失败 → 重建；必要时保留损坏文件备份（`*.corrupt.<ts>`）。
3. **版本不兼容**：meta 不匹配 → 重建。
4. **重建失败**：
   - Vector：禁用向量索引能力（回退到无索引）；`vec_similarity` 仍可在行级运行（代价是扫描）。
   - FTS：`txt_score` 返回 0/Null 并记录错误；同时保持 DB 其它能力可用（不让核心路径挂掉）。

## 8. Testing Strategy

### 8.1 Unit / Integration

- Vector
  - 向量属性 roundtrip（FlexBuffers）+ `vec_similarity` 正确性（维度不匹配、Null、空向量）
  - 索引重建：删 sidecar → open → rebuild → 查询结果一致
  - 事务一致性：begin → 写向量 → abort → 索引不包含；begin → 写向量 → commit → 索引包含
- FTS
  - 文本属性写入 → `txt_score` 命中/不命中语义固定
  - `flush_indexes()` 前后可见性验证（NRT）

### 8.2 Crash / Recovery

Sidecar 不是 Source of Truth：即使 crash 导致 sidecar 不完整，也必须能通过重建恢复。

## 9. Rollout Plan

1. 先落地 T33（feature gate + sidecar + rebuild + API + 函数）。
2. 再评估“无改语法的索引加速”：
   - planner 识别 `txt_score(...) > t` / `vec_similarity(...) > t` 的可优化模式
   - 或新增 row-generator 函数配合 `UNWIND`（不改 MATCH 语法也能用索引做候选集）。

