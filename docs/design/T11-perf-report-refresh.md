# T11: 性能重测与报告刷新（基于 T10 stmt API）

## 1. Context

当前 `docs/PERFORMANCE_ANALYSIS.md` 的数字已经过时，且 `redb (raw)` 基线基准存在明显方法论问题：

- `bench_compare.rs` 的 `bench_redb()` 在插入循环内大量 `format!()` 拼接字符串键值，测到的是**字符串分配/格式化**，不是 `redb` 的写入能力。
- 结果出现“封装层（NervusDB）比 raw redb 更快”的反直觉现象，报告可信度为 0。
- T10 已引入 SQLite 风格 `prepare_v2/step/column_*` 的 C statement API，但性能报告未覆盖该路径。

这一步不加新功能，只做一件事：把基准测量变得可信，并把报告更新到当前实现的真实水平。

## 2. Goals

- 刷新 `docs/PERFORMANCE_ANALYSIS.md`：更新到最新实现（含 LRU interning、T10 stmt API）的数据。
- 修正 `bench_compare.rs` 的 `redb (raw)` 方法论：去掉插入热路径的 `format!/to_string` 噪音，改为用与 NervusDB 相同的数据结构（`(u64,u64,u64)->()` 三索引表 + range 扫描）。
- 补充 T10 的性能对比：至少给出 `exec_cypher(JSON)` vs `stmt(step/column)` 的同形查询对比，明确当前实现仍是 eager（非 streaming）但避免 JSON 文本序列化/解析。

## 3. Non-Goals（明确不做）

- 不改核心存储/事务/FFI ABI（`nervusdb.h` 冻结不动）。
- 不把 Node N-API 改成走 stmt（1.0 收敛阶段允许它继续构造 JS 对象，但必须在报告里说清楚成本在哪里）。
- 不把 long-running bench 放进 PR CI（CI 只做 correctness/smoke）。

## 4. Solution

### 4.1 修正 redb 基线（核心点）

在 `nervusdb-core/examples/bench_compare.rs`：

- `bench_redb()` 改为三张 `TableDefinition<(u64,u64,u64), ()>`：
  - `spo`: `(s,p,o)`
  - `pos`: `(p,o,s)`
  - `osp`: `(o,s,p)`
- 插入阶段只写三表，不做字符串拼接。
- 查询阶段用 `range((s,MIN,MIN)..=(s,MAX,MAX))` / `range((o,MIN,MIN)..=(o,MAX,MAX))`，与 `DiskHexastore::plan()` 一致。

### 4.2 T10 stmt 性能对比

新增/扩展一个 **手动运行** 的基准入口（优先复用现有 example，避免引入新依赖）：

- 固定数据集（与 `bench_compare` 同数量级，但查询行数可单独配置）
- 对比：
  - `nervusdb_exec_cypher`（JSON 字符串输出）
  - `nervusdb_prepare_v2 + while(step==ROW){column_*}`（stmt）
- 结果写入报告并明确：
  - stmt 当前是 eager 收集结果再迭代（优化点留到 1.1+）
  - 但已经避免 JSON 文本构造/解析，且建立了 ABI 契约

### 4.3 报告写法（防止再次变成垃圾）

`docs/PERFORMANCE_ANALYSIS.md` 必须包含：

- Last updated（日期）
- 运行环境（CPU/OS/磁盘，至少写“本机/CI 不同会波动”）
- 每个数字“包含什么/不包含什么”（例如：NervusDB 插入不含 interning；Node 查询仍会构造 JS 对象）

## 5. Testing Strategy

- 本地跑：
  - `cargo run --example bench_compare -p nervusdb-core --release`
  - 如有新增 stmt 基准：对应命令跑一次并记录输出
- CI：不跑长基准，只确保 `cargo test` / Node tests / crash-smoke 仍绿。

## 6. Risks

- 基准仍可能出现“比谁更会写 benchmark”的伪结论：需要在报告中写清楚测量边界，避免过度解读。
- 数据分布单一（每个 subject/object 只命中 1 行）会夸大点查性能：报告必须注明这是点查基准，不代表图遍历/聚合。

