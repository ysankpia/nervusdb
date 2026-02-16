# NervusDB v2 架构交叉验证文档（独立版）

> 目的：提供一份与 `nervusdb-Architecture.md` 同章节结构、但独立事实来源的架构文档，用于交叉验证。
> 方法：事实栏仅依据代码、测试脚本、CI 工作流、产物统计；优化栏给独立方案并标注前置依赖。
> 生成时间：2026-02-11（本地仓库快照）。

---

# 第一部分：现状架构（独立事实）

## 1. 项目概述

### 事实栏（As-Is）
- 当前工程治理口径已进入 SQLite-Beta 收敛线，发布判定依赖 TCK/稳定窗/SLO，而不是 M3 Alpha 里程碑叙事。
- 当前 Tier-3 全量通过率基线为 `81.93%`（`3193/3897`，生成时间 `2026-02-11T09:17:30Z`）。
- 查询引擎仍存在 `NotImplemented` 路径，当前计数为 9（`parser 1 + query_api 2 + executor 6`）。
- 存储格式兼容已启用 epoch 强校验，不匹配会报 `StorageFormatMismatch`。

### 优化栏（To-Be）
- 在文档首章拆分“历史里程碑口径”和“当前发布口径”两个固定区块，避免历史叙事误导当前决策。
- 将 TCK 通过率改为“自动注入字段”（由 `artifacts/tck/tier3-rate.json` 同步），避免手写过时。
- 将 NotImplemented 计数改为“脚本实时统计”并在 PR 模板显示差值。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:18`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:2`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:11`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/parser.rs:1127`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:834`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:925`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:3467`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:3983`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:4776`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:5442`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:5446`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:6235`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/pager.rs:103`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/error.rs:17`

## 2. Workspace 结构

### 事实栏（As-Is）
- 根 workspace 当前成员包含 `nervusdb-cli`、`nervusdb-pyo3`、`nervusdb`、`nervusdb-api`、`nervusdb-query`、`nervusdb-storage`。
- `nervusdb-node` 不在根 workspace 列表中，采用独立 `Cargo.toml` 路径构建方式。
- `fuzz` 是独立 fuzz workspace，直接依赖 `nervusdb-query`。

### 优化栏（To-Be）
- 保持“核心 workspace + 外挂 workspace（node/fuzz）”策略，但在根 README 固化边界说明与统一命令入口。
- 为 node/fuzz 增加 `make` 包装目标，避免脚本散落导致操作不一致。
- 为 workspace 引入机器可读元数据（例如 `docs/refactor/workspace-map.md`）减少认知负担。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/Cargo.toml:2`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/Cargo.toml:9`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/fuzz/Cargo.toml:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/fuzz/Cargo.toml:12`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/fuzz/Cargo.toml:35`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-node/src/lib.rs:4`

## 2.5 发布策略

### 事实栏（As-Is）
- 当前 crate 命名仍带 `-v2` 后缀，尚未完成包名收敛。
- 主包 `nervusdb` 已 re-export 常用类型与 query crate，但未公开 re-export `GraphStore` trait。
- CLI 仍直接依赖并使用 `nervusdb-storage` 的 `GraphEngine`，未完全收敛到仅依赖门面层。
- Python/Node 错误模型已开始向 `Syntax/Execution/Storage/Compatibility` 靠拢。

### 优化栏（To-Be）
- 发布策略拆成两条：
- 一条是“可用性发布策略”（当前保留 `-v2`，不阻断 Beta）。
- 一条是“命名收敛策略”（等 Beta 稳定窗达标后再进行 package rename）。
- 在门面层补齐 `GraphStore`、维护接口和运维能力的公开边界，减少 CLI/绑定层穿透。
- 将 bindings 错误 payload 结构定义升级为契约文档并接入 contract smoke。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/Cargo.toml:2`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:50`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:58`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/Cargo.toml:21`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:6`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:5`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/src/lib.rs:30`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-node/src/lib.rs:46`

## 3. 整体架构

### 事实栏（As-Is）
- 当前分层仍是 `api -> storage -> query -> facade/cli/bindings`，依赖方向总体可读。
- `Db::open` 最终落到 `GraphEngine::open`，门面层本身较薄。
- query 执行仍是 `prepare -> execute_plan/execute_write`，尚未引入独立 planner/optimizer 模块树。

### 优化栏（To-Be）
- 显式定义“门面层唯一入口”策略：CLI 与 bindings 只通过门面调用核心能力。
- 为 query 引擎引入“编译层接口（planner boundary）”而非直接改 executor，先把耦合点可视化。
- 在架构图中区分“当前实现链路”和“目标链路”，防止同图混写。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:92`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:108`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:33`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:40`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:108`

## 4. 存储引擎（nervusdb-storage）

### 事实栏（As-Is）
- Pager 页大小固定 8KB，bitmap 仍是单页位图模型，容量上限由 `BITMAP_BITS` 推导。
- Meta 已包含 `storage_format_epoch`，并在打开时强校验。
- GraphEngine 内部使用 `Arc<RwLock<Pager>>`、`Mutex<Wal>`、`Mutex<IdMap>`、`Arc<Mutex<IndexCatalog>>` 组合。
- WAL 同时记录事务边界、页写和图语义写入记录。
- `create_index` 当前明示“不回填历史数据”，只覆盖创建后写入。
- Storage crate 目前不存在 `buffer_pool`、`vfs`、`label_index` 等模块声明。

### 优化栏（To-Be）
- 先在不改文件格式前提下完成“读路径分层”：把索引读取、属性读取、快照读取接口边界拉开。
- 再推进 bitmap 扩展与缓冲池能力，避免把“结构重构”和“格式变更”绑定为同一任务。
- 索引回填建议走后台批任务+可见状态位，而非同步阻塞式 create。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:21`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/pager.rs:31`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/pager.rs:45`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/pager.rs:103`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:44`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:48`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/wal.rs:10`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:164`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:17`

## 6. 并发模型

### 事实栏（As-Is）
- 写事务由 `write_lock` 串行化。
- `snapshot()` 路径包含 `scan_i2e_records()`，会构造全量 `i2e` 拷贝。
- StorageSnapshot 读属性需要 `pager` 读锁，索引查询需要 `index_catalog` 互斥锁。

### 优化栏（To-Be）
- 优先把 `i2e` 发布模型改为 `Arc` 快照发布，降低 snapshot 创建的复制成本。
- 将 `index_catalog` 查询路径改为读优化结构，减少查询热点锁竞争。
- 在不改外部 API 的前提下引入读路径缓存层，先改抽象再改性能策略。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:57`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:303`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:29`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:20`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:21`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:97`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:110`

## 7. 索引系统

### 事实栏（As-Is）
- 当前索引模块为 `btree`、`catalog`、`hnsw`、`ordered_key`、`vector`。
- `IndexCatalog` 是单页目录，页面满会报错。
- 当前 `create_index` 不执行历史数据回填。
- 代码层面未见标签索引与全文索引模块声明。

### 优化栏（To-Be）
- 索引能力分三步：
- 第一步实现回填与状态机（Building/Online）。
- 第二步引入标签位图索引以承接高频 label 过滤。
- 第三步再考虑全文索引，避免同时引入多套写放大路径。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/mod.rs:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/mod.rs:5`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/catalog.rs:18`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/catalog.rs:147`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:164`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:17`

## 8. 数据模型

### 事实栏（As-Is）
- API 层定义了 `ExternalId=u64`、`InternalNodeId=u32`、`EdgeKey(src,rel,dst)`、`PropertyValue`。
- Storage 层仍保留一套 `PropertyValue` 与 `snapshot::EdgeKey`，并在 API 适配层做转换。
- 当前边主键仍为 `(src,rel,dst)` 语义，没有独立 `EdgeId`。

### 优化栏（To-Be）
- 先完成 api/storage 的类型收敛，减少转换与重复编码路径。
- 多重边能力（EdgeId）放在后置阶段，必须与 WAL/CSR/B-Tree 迁移计划打包。
- 绑定层模型升级应与核心模型升级解耦，先做兼容适配层。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs:7`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs:13`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs:38`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs:84`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/property.rs:5`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/snapshot.rs:10`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:54`

## 9. 现状评估

### 事实栏（As-Is）
- 优势：分层清晰、WAL+checkpoint 模型明确、HNSW/Bindings/CI 门禁链路均已落地。
- 风险：query 三大核心文件仍是单体实现，重构需求已进入 tasks 主线。
- 风险：CLI 直接打开 `GraphEngine`，与门面层并行打开同库路径存在边界治理风险。
- 风险：发布口径、文档口径和代码实现仍有局部漂移，需要“事实源优先”统一收敛。

### 优化栏（To-Be）
- 按“行为等价优先”执行结构拆分，先降低回归半径再推进功能簇。
- 将 CLI 边界收敛为门面唯一入口，避免双路径访问。
- 建立文档自动校验：关键指标来自 artifacts，不允许手填。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:100`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:101`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:102`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:210`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:211`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:44`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:46`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/ci.yml:44`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/ci.yml:71`

---

# 第二部分：优化架构方案（独立方案）

## 10. 重构总览

### 事实栏（As-Is）
- 当前主约束是 Beta 门禁：TCK≥95%、7天稳定窗、SLO 封板。
- 当前任务板已明确把 query 三大文件拆分列为 Beta 子任务。

### 优化栏（To-Be）
- 重构目标分三层：
- 第一层：可维护性（模块拆分、边界收敛）。
- 第二层：正确性（契约一致、错误模型稳定）。
- 第三层：性能（并发热点与索引回填）。
- 原则：先结构等价，再性能与能力扩展。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:18`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:20`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:100`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:101`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:102`

## 11. 查询引擎重构

### 事实栏（As-Is）
- Query crate 当前模块仍集中在 `query_api.rs`、`executor.rs`、`evaluator.rs` 三大单体文件。
- `prepare()` 已承担较重职责，执行路径直接连接 executor。
- 当前仍存在 NotImplemented 分支，说明“功能清簇”和“结构拆分”要并行治理。

### 优化栏（To-Be）
- 阶段 1：仅拆分文件和内聚边界，不改语义。
- 阶段 2：引入 planner 边界（先逻辑接口，再规则优化）。
- 阶段 3：将 temporal/aggregation/projection 等高变更区拆到独立子模块。
- 每次拆分只做一个子域，必须先锁定回归集（feature + tier0/1/2）。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:35`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:36`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:40`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:73`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:108`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/error.rs:10`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:100`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:103`

## 12. 存储引擎增强

### 事实栏（As-Is）
- 当前代码内尚未实现 BufferPool/VFS/LabelIndex 模块。
- `BITMAP_BITS` 固定，容量扩展仍未落地。
- 索引目录仍是单页 catalog，达上限会触发 page full 错误路径。

### 优化栏（To-Be）
- 增强顺序建议：
- 先做 bitmap 容量扩展与元数据兼容。
- 再做读路径缓存和局部并发优化。
- 最后做 VFS 抽象与存储策略替换。
- 所有 storage 增强必须在 `storage_format_epoch` 语义下显式声明兼容策略。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:17`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/pager.rs:31`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/catalog.rs:147`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/error.rs:17`

## 13. 并发模型改进

### 事实栏（As-Is）
- 当前模型是“单写 + 多读”，但读路径并非完全无锁。
- Snapshot 构造和属性/索引读取仍触发锁竞争点。

### 优化栏（To-Be）
- 把并发改进拆成三步：
- 第一步：发布态数据结构（`Arc` 快照）改造。
- 第二步：索引查询与属性读取分层，减少全局锁参与。
- 第三步：按热点引入局部缓存与读写隔离策略。
- 并发优化必须绑定可重复基准（读热点、混合读写、长查询）。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:44`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:57`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:29`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:110`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:90`

## 14. 数据模型增强

### 事实栏（As-Is）
- API 与 Storage 的 PropertyValue/EdgeKey 重复定义仍存在。
- query 运行时 `Value` 包含运行期实体语义，无法直接下沉到 API 层。

### 优化栏（To-Be）
- 先做 API/Storage 类型统一，保留 Query Runtime Value 独立存在。
- 针对 EdgeId 的多重边支持，单列为“破坏性格式迁移”里程碑，不与拆分任务混合提交。
- 绑定层输出模型保持兼容，新增字段走可选扩展。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs:38`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-api/src/lib.rs:84`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/property.rs:5`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/snapshot.rs:10`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs:164`

## 15. 索引增强

### 事实栏（As-Is）
- 当前索引 create 流程不做 backfill。
- catalog 与 btree 已可工作，但缺少在线状态机与分级维护能力。
- 当前仍无标签索引/全文索引实现模块。

### 优化栏（To-Be）
- 引入索引状态机：`Building -> Online -> Failed`。
- 回填流程应可中断、可恢复、可观测，不阻塞前台写事务。
- 标签索引优先于全文索引，先解决主路径过滤性能。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:164`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/catalog.rs:20`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/mod.rs:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/index/mod.rs:5`

## 16. 重构优先级与路线图（4-6 周，可执行）

### 事实栏（As-Is）
- 当前门禁链路已可执行并在 CI 中生效。
- tasks 已有 Query 拆分与 Beta 目标任务，具备落地入口。

### 优化栏（To-Be）
- **Phase 0（第1周）审计与护栏**：冻结事实基线、建立回归集、统一证据口径。
- **Phase 1（第2-3周）Query 结构拆分**：`query_api`/`executor`/`evaluator` 分治，严格行为等价。
- **Phase 2（第3-4周）边界收敛**：CLI 仅走门面，补齐 API re-export，完成 bindings 契约校验。
- **Phase 3（第5周）Storage 增强起步**：回填框架、快照路径优化、容量扩展设计落地。
- **Phase 4（第6周）收敛验收**：全门禁、回归差分、风险清单清零或入账。
- 每个阶段都必须附：目标、前置条件、门禁、回滚条件。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/ci.yml:44`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/ci.yml:48`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/ci.yml:50`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/ci.yml:62`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/.github/workflows/tck-nightly.yml:36`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/workspace_quick_test.sh:7`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/tck_tier_gate.sh:87`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/binding_smoke.sh:7`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/contract_smoke.sh:7`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:99`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:100`

## 17. 重构后的项目结构（目标态）

### 事实栏（As-Is）
- 当前 query/storage 仍以单文件核心实现为主，模块树扁平。
- 当前根目录采用多 crate 平铺结构，尚未执行目录级重组。

### 优化栏（To-Be）
- 推荐保守结构重组：
- 保持根级 crate 不变。
- 在 `nervusdb-query/src` 内先完成 `planner/`、`executor/`、`evaluator/` 子目录拆分。
- 在 `nervusdb-storage/src` 内先新增 `read_path/` 与 `indexing/` 子域，不强制一次性迁移。
- 目标是降低回归半径，不追求一次性“漂亮目录”。

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:33`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs:40`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:1`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:17`

## 18. 关键设计决策对照

### 事实栏（As-Is）
- 当前系统是“可运行 Beta 收敛态”，不是“架构目标态”。
- 当前主要问题是可维护性和边界一致性，而非单点功能缺失。

### 优化栏（To-Be）
| 维度 | 当前事实 | 独立优化决策 | 风险控制 |
|---|---|---|---|
| Query 组织 | 三大单体文件 | 先模块拆分，再 planner 引入 | 每 PR 全门禁 |
| CLI 边界 | 直连 GraphEngine | 收敛到 Db 门面 | 分支级回滚 |
| 类型模型 | api/storage 重复 | 先统一类型，再迁移 EdgeId | 兼容测试兜底 |
| 索引能力 | 无回填/无标签索引 | 先回填状态机，再标签位图 | 可恢复构建任务 |
| 并发路径 | snapshot 拷贝+锁热点 | 发布态快照+读路径分层 | 基准对比门禁 |
| 文档治理 | 手工口径漂移 | 指标自动注入 | artifacts 单一事实源 |

### 证据
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs:73`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:211`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/api.rs:29`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/engine.rs:164`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:11`

---

## 冲突清单（用于与 `nervusdb-Architecture.md` 交叉验证）

| ID | 冲突点 | 本文独立结论 | 证据 |
|---|---|---|---|
| C1 | 当前里程碑口径 | 当前应按 Beta 收敛线表达，不应写成 M3 Alpha 当前态 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:1`, `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:99` |
| C2 | TCK 当前值 | 当前基线是 `81.93%`（2026-02-11） | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/artifacts/tck/tier3-rate.json:11` |
| C3 | 门面层边界 | CLI 仍有直连 storage 的实现，不是纯门面接入 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/main.rs:6`, `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-cli/src/repl.rs:5` |
| C4 | 主包 re-export 完整性 | `GraphStore` 目前未 `pub use` | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:50`, `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/src/lib.rs:58` |
| C5 | Storage 增强落地状态 | BufferPool/VFS/LabelIndex 当前尚未落地模块声明 | `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:1`, `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-storage/src/lib.rs:17` |

## 优先级执行矩阵（可直接映射 `docs/refactor/`）

| 优先级 | 任务 | 输入 | 产出 | 门禁 | 回滚条件 |
|---|---|---|---|---|---|
| P0 | Query 三件套拆分（行为等价） | 现有 query 三大文件 | 子模块结构 + 等价回归 | fmt + clippy + quick + tier0/1/2 + bindings + contract | 任一门禁失败立即回滚 PR |
| P0 | CLI 边界收敛 | CLI 当前双路径调用 | CLI 仅门面调用 | 同上 + CLI 回归 | 查询/写入行为变化 |
| P1 | 类型模型收敛（api/storage） | 重复类型定义与转换 | 减少重复与转换胶水 | 单测 + contract smoke | bindings 输出不兼容 |
| P1 | 索引回填状态机 | 当前 create_index 语义 | Building/Online 流程 | tier0/1/2 + 定向回归 | 回填导致写阻塞异常 |
| P2 | 并发读路径优化 | snapshot 与锁热点 | 发布态快照 + 读路径分层 | quick + 基准回归 | 延迟与吞吐回退 |
| P2 | 容量扩展与VFS | 单页 bitmap + 无VFS | 容量扩展方案与抽象层 | 存储回归 + 兼容验证 | 格式兼容风险未闭环 |

---

> 本文档仅用于交叉验证与重构执行，不代表代码已完成对应优化项。优化栏中的改动均为候选方案，默认未实施。
