# Hypothesis: SQLite-Like Embedded Graph Database Architecture

## 1. 文档定位

本文件是一个**假设性架构方案**，用于讨论“图数据库界的 SQLite”目标下，嵌入式图数据库应采用的实际工程架构。  
该方案不依赖当前仓库实现细节，不代表已实现功能，仅作为设计参考。

## 2. 目标与原则

### 2.1 产品目标

- 单机嵌入式（in-process）优先
- 零运维体验（默认配置即开即用）
- 单文件分发与迁移
- 高可靠性（断电可恢复）
- 长期文件格式兼容

### 2.2 设计原则

- 可靠性优先于极限吞吐
- 可调试性优先于黑箱优化
- 默认简单，按需启用高级特性
- 核心内核最小化，扩展能力插件化

## 3. 总体架构

```text
Application
  -> Embedded API Layer (Rust/C ABI/Python/Node)
    -> Query Layer (Parser -> IR -> Optimizer -> Executor)
    -> Txn Layer (SWMR + Snapshot Read)
      -> Storage Kernel (Pager + Record Store + Index + WAL)
        -> Files (main.db + main.wal + checkpoints)
    -> Extension Layer (FTS/Vector/Graph Algorithms)
```

## 4. 分层模块设计

### 4.1 API Layer

- Rust 原生 API（首选）
- 稳定 C ABI（对外兼容层）
- Python/Node/Java 等语言绑定基于 C ABI
- 三层接口：
  - 低层内核 API（页、记录、索引维护）
  - 中层 Graph API（CRUD、事务、批量导入）
  - 高层 Query API（Cypher/GQL 子集）

### 4.2 Query Layer

- 语法：openCypher / GQL 子集
- Pipeline：
  - Parser -> AST
  - AST -> Logical IR
  - Rule Rewrite + Cost Optimizer
  - Physical Plan -> Executor
- 执行模型：Volcano iterator（后续可扩展向量化）
- 可观测性：内建 `EXPLAIN` / `EXPLAIN ANALYZE`

### 4.3 Transaction Layer

- 默认并发模型：SWMR（Single Writer Multiple Readers）
- 读事务：快照可见性（Repeatable Read 默认）
- 写事务：数据库级写锁（后续可扩展细粒度分区锁）
- 隔离等级：
  - Read Committed
  - Repeatable Read
  - Serializable（可选严格模式）

### 4.4 Storage Kernel

- 页式存储：默认 4KB（可配 8KB/16KB）
- 页结构：slotted page 管理可变长记录
- 记录编码：tagged binary（TLV 风格）
- 数据组织：
  - NodeRecord：id/labels/properties/adjacency pointers
  - EdgeRecord：id/src/dst/type/properties
- 邻接结构：小度链式 + 大度压缩块混合策略
- 空间管理：free-list + 位图元数据

### 4.5 WAL & Recovery

- 默认 WAL 模式
- 日志粒度：page-delta + logical redo 混合
- 恢复策略：简化 ARIES（LSN + checkpoint + redo/undo）
- Checkpoint：按时间和日志大小双阈值触发
- Vacuum：
  - 在线渐进整理
  - 离线全量重写

### 4.6 Index Layer

- 属性索引：B+Tree
- 标签与类型索引：倒排
- 邻接加速索引：`(src,type)` / `(dst,type)` 组合
- 约束：
  - 唯一约束
  - 存在性约束
- 统计信息：
  - label cardinality
  - property histograms
  - degree distribution

### 4.7 Extension Layer

- 能力插件化（capability-based）
- 非核心能力以扩展模块交付：
  - Full-text Search
  - Vector Index / ANN
  - 图算法库（PageRank、社区检测等）
- 核心引擎不绑定网络协议与分布式特性

## 5. 数据与文件兼容策略

### 5.1 文件形态

- `main.db`：主数据库文件
- `main.wal`：写前日志文件
- 可选快照备份文件：`*.snapshot`

### 5.2 文件头元信息

- `format_version`
- `feature_flags`
- `page_size`
- `checksum`
- `compat_matrix_id`

### 5.3 升级策略

- 允许新版本读取旧格式
- 写入新特性前需显式 `upgrade` 动作
- 禁止隐式破坏式迁移

## 6. 可靠性与工程质量基线

### 6.1 测试体系

- 单元测试（核心逻辑）
- 属性测试（边界与随机输入）
- 差分测试（与参考实现对比）
- 崩溃注入测试（fsync/电源中断场景）
- 模糊测试（解析与执行）
- soak 稳定性测试（长时间）

### 6.2 关键验证维度

- 跨版本读写兼容
- WAL 恢复一致性
- 索引重建一致性
- 查询结果稳定性
- 数据文件损坏检测与报错可解释性

### 6.3 可观测性最小集合

- Page cache hit ratio
- WAL 写入与 fsync 延时
- Checkpoint 耗时
- 慢查询 trace
- 查询计划与实际行数偏差

## 7. 明确不进入核心范围（Out of Core）

- 分布式一致性协议
- 多副本复制与跨机容灾
- 复杂多租户权限系统
- 默认网络服务端能力

以上能力可由外围服务层或独立网关实现。

## 8. 分期落地路线（Hypothesis）

### P1: 可用内核

- 事务（SWMR + WAL）
- 基础图 CRUD
- 核心查询子集（MATCH/WHERE/RETURN）
- 基础索引（B+Tree + label/type）

### P2: 性能与优化

- 代价优化器
- 统计信息收集
- 邻接混合结构增强
- 在线 checkpoint / vacuum 策略完善

### P3: 生态与稳定性

- 多语言绑定完善
- 扩展插件体系稳定化
- 文件兼容矩阵与长期发布治理
- 端到端观测与诊断工具

## 9. 结论

该假设方案的核心是：

- 稳定单文件引擎
- 图专用查询执行层
- 可插拔扩展能力
- 严格兼容与恢复治理

这组组合最接近“图数据库界 SQLite”的目标：简单可嵌入、稳定可依赖、长期可维护。

---

附注：本方案由一次不少于 50 步的 sequential 推演收敛得到，用于设计讨论与路线评审。
