# NervusDB v2 — 产品规格（Spec v2.0）

> 这份 spec 是 v2 的“宪法”：写清楚约束和边界，避免靠幻想写代码。
>
> **范围声明**：
>
> - 本 spec 约束 NervusDB v2（新 crate / 新磁盘格式 / 不兼容 v1）
> - 路线图（愿景/规划）在 `docs/ROADMAP_2.0.md`，不要把“计划”当“已实现”

## 1. 产品定位（Product Identity）

- **一句话使命**：做一个纯 Rust 的嵌入式 Property Graph 数据库，像 SQLite 一样“一个文件打开就能用”，但为图遍历而生。
- **核心用户路径**：本地打开 DB → 写入 → 查询（流式）→ 崩溃恢复可验证。

## 2. 现状（仓库事实）

这些是“已经存在”的东西；其他任何内容都必须明确标注为“计划”。

- 存储：`.ndb + .wal`（Pager `RwLock` + Offset IO + redo WAL），页大小固定 `8KB`，支持 Vacuum
- 一致性：Single Writer + Snapshot Readers（快照读并发）
- 崩溃恢复：WAL replay + manifest/checkpoint（crash gate）
- 图数据：MemTable → L0 frozen runs → 多段 CSR segments（compaction）
- 查询：v2 query 最小子集（以 `docs/reference/cypher_support.md` 白名单为准）

## 3. 硬约束（Constraints）

- **兼容性**：v2 不兼容 v1（文件格式/接口/实现都独立）；v1 继续存在但不是 v2 的包袱
- **外部依赖**：零外部服务进程（不需要 daemon）
- **平台**：Native 优先（Linux/macOS/Windows）；WASM 仅内存实现
- **安全性**：不硬编码 secrets；崩溃一致性是硬门槛
- **复杂度纪律**：不要为了“理论完美”引入不可控复杂度；能用最蠢的清晰做法就别耍花活

## 4. v2.0 路线（Roadmap 2.0: Planned）

以下是“计划”，不是仓库事实。具体拆解见：

- 路线图：`docs/ROADMAP_2.0.md`
- 架构总览：`docs/design/T100-v2-architecture-2.0.md`

### 4.1 Indexing（计划）

- 在 `.ndb` Pager 内引入 **Page-backed B+Tree** 二级索引
- 初期只做“页布局 + cursor + 有序 key 编码”（T101），避免一口吃成胖子
- 索引目录 / 多索引管理 / compaction 集成属于后续任务（T102/T103）

### 4.2 Query（计划）

- `EXPLAIN`、`MERGE` 等按任务推进（T104/T105…）
- 优化器/索引选择属于后续，必须可测试、可回滚

### 4.3 Lifecycle（计划）

- “Single File at Rest”（Checkpoint-on-Close）属于后续（T106）

## 5. 测试策略（Testing Strategy）

- 单元测试：pager/wal/idmap/memtable/index page layout（T101 相关）
- 集成测试：storage + query 端到端（最小子集）
- 崩溃门禁：CI crash-gate（PR 小跑 + 定时大跑）

## 6. 已知技术债与折衷（Technical Debt & Trade-offs）

- **暂无**：目前主要技术债（如 Vacuum, B-Tree 删除, 执行器动态分发）已在 v2.0 (T204-T207) 阶段解决。
