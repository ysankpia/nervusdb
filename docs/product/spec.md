# NervusDB v2 — 产品规格（Spec v0.1）

> 这是 v2 的“宪法”。不写清楚这份东西，开发就会变成无止境的功能堆砌。
>
> **范围声明**：本 spec 仅约束 NervusDB v2（新 crate / 新磁盘格式 / 不兼容 v1）。

## 1. 产品概览（Product Overview）

- **一句话使命**：做一个纯 Rust 的嵌入式 Property Graph 数据库，像 SQLite 一样“一个文件打开就能用”，但为图遍历而生。
- **目标用户**
  - Rust 应用开发者（本地/桌面/边缘/服务端内嵌）
  - 需要“零部署、零外部依赖”的图存储场景（CLI/SDK/嵌入式）
  - 多语言绑定使用者（Node/Python/C/WASM）——但 v2 的核心交付仍以 Rust 为准
- **核心痛点**
  - 传统 KV-on-Graph（v1）在深度遍历/邻居枚举上有物理瓶颈（树查找 vs 顺序扫描）
  - 现有图数据库通常需要服务进程/复杂部署，不适合嵌入式
  - 需要“崩溃安全 + 可预测的 IO + 可流式消费结果”的基础设施

## 2. 功能需求（Functional Requirements）

### 2.1 MVP（Must Do）

**存储与事务**
- [x] `.ndb + .wal` 两文件：page store + redo WAL（不强求单文件）
- [x] 事务模型：Single Writer + Snapshot Readers（读快照并发，写全局串行）
- [x] 崩溃恢复：WAL replay + manifest/checkpoint（可通过 crash gate）
- [x] 写入能力：CreateNode / CreateEdge / TombstoneNode / TombstoneEdge
- [x] 读取能力：`neighbors(src, rel)` + `nodes()`（全表扫描）+ `resolve_external()` + `node_label()`

**查询（v2 M3 最小子集）**
- [x] `RETURN 1`
- [x] 单跳匹配：`MATCH (n)-[:<u32>]->(m) RETURN n, m LIMIT k`
- [x] 结果必须是 streaming（iterator），禁止 `collect()` 成全量 Vec

**CLI 验收路径**
- [x] `nervusdb v2 query --db <path> --cypher/--file ...` 输出 NDJSON（每行一条记录）

### 2.2 v2.x（Optional / Post-MVP）

这些不是“以后再说”，而是明确不属于 MVP 的范围（否则你会死在边缘情况里）：

- Cypher：WHERE/CREATE/MERGE/多跳/OPTIONAL MATCH/聚合/排序等（逐项任务化）
- Label/RelType/String 字典：`String -> u32` intern（持久化与 cache）
- 属性：WAL/MemTable 写入 + compaction 时 columnar 固化 + 读取/过滤下推
- 二级索引（B+Tree/属性索引）、向量索引（HNSW）、全文索引（可选 feature）
- 多 label / schema 管理
- 多语言绑定的“稳定 ABI 契约”（v2 专属，不继承 v1）

## 3. 架构决策（Architectural Decisions）

> 这里写死的是“不会轻易改”的核心。你想改，先写设计文档再说。

- **存储引擎**：自研 Pager（8KB page）+ LSM 风格图存储
  - L0：MemTable（内存 delta）
  - L0 frozen runs：commit 冻结为不可变段（Arc 持有）
  - L1+：不可变 CSR segments（多段，不维护全局完美 CSR）
- **一致性**：redo WAL（默认每次 commit fsync；可配置 durability 等级）
- **隔离级别**：单写者 + 快照读（snapshot isolation）
- **查询边界**：Query/Executor 只能依赖 `nervusdb-v2-api::{GraphStore, GraphSnapshot}`，不得窥探 pager/WAL/segments
- **WASM**：MVP 只保证 in-memory engine（磁盘格式不共享）

## 4. 约束（Constraints）

- **兼容性**：v2 不兼容 v1（文件格式/接口/实现都独立）；v1 继续存在但不是 v2 的包袱
- **外部依赖**：零外部服务进程（不需要 daemon）
- **平台**：Native 优先（Linux/macOS/Windows）；WASM 仅内存实现
- **安全性**：不硬编码任何 secrets；崩溃一致性是硬门槛
- **复杂度纪律**：MVP 禁止引入“看起来很高级但不可控”的后台 IO/compaction 线程（优先显式 API）

## 5. 测试策略（Testing Strategy）

- **单元测试**：pager/wal/idmap/memtable/query parser/executor
- **集成测试**：v2-storage + v2-query 端到端（最小子集）
- **崩溃门禁**：CI crash-gate（PR 小跑 + 定时大跑）
- **性能门禁**：v2 bench/perf gate（回归可见）
- **验收原则**：能跑通用户路径（CLI + streaming + crash-safe）比“语法覆盖率”更重要

## 6. 里程碑与“什么时候算完”（Milestones / Definition of Done）

### 6.1 已完成（当前仓库事实）

- M0/M1/M2：v2 内核、compaction、durability、crash gate、bench gate 已落地
- M3：v2 query 最小子集 + API + CLI 验收路径已落地

### 6.2 v2.0.0-alpha1（本阶段收敛目标）

只要满足以下条件，就可以宣布 **alpha1 完结**（不再加功能，转入稳定性修正）：

- [ ] CI 全绿（含 crash-gate-v2）
- [ ] CLI：`nervusdb v2 query` 能在空库/小库上稳定输出 NDJSON
- [ ] 查询结果 streaming：大结果集不会爆内存（不允许隐式 collect）
- [ ] 明确并冻结 v2 的“最小 Cypher 子集”清单（超出即 NotSupported）
- [ ] 文档：README/CHANGELOG 明确 v2 现状与限制（不吹牛）

### 6.3 v2.0.0（正式版的最低门槛，先写死）

> 这不是现在就做完，但这是“什么时候是个头”的唯一答案：达到它就停。

- [ ] 稳定的公开 Rust API（`nervusdb-v2` facade + `nervusdb-v2-query`）
- [ ] Cypher：至少支持基础读写闭环（CREATE/MATCH/WHERE/RETURN/LIMIT）——每项必须有测试
- [ ] 数据一致性：crash gate、恢复语义、tombstone/compaction 语义都被测试锁死
- [ ] 性能：提供基准与对比方法（不需要赢所有人，但要可重复、可解释）

## 7. 待确认项（需要你拍板，不拍板我就按默认执行）

1. v2 对外 ID：是否只支持 `ExternalId=u64`（当前实现是），还是 MVP 就要支持 string？
2. v2 的“公开入口”是否以 `nervusdb-v2`（事务/DB）+ `nervusdb-v2-query`（prepare/execute）为唯一官方路径？
3. alpha1 的发布口径：你希望对外宣称“已经支持哪些 Cypher 子集”？（建议严格白名单）

