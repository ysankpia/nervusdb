# NervusDB v2 项目状态评估报告

> 生成时间：2025-01-XX  
> 基于 Gemini 讨论和代码审查

## 执行摘要

**核心结论**：v2 存储引擎架构已完成（M0-M2），但查询层功能严重缺失，导致产品价值暂时倒退。**必须立即将重心转向基础查询闭环，而非继续打磨存储引擎。**

## 一、当前状态分析

### 1.1 v1 版本（已完成，稳定）

- ✅ **功能完整**：支持完整的 Cypher 查询子集（CREATE/MATCH/WHERE/RETURN/LIMIT/聚合/排序等）
- ✅ **多语言绑定**：Node.js/Python/C/WASM
- ✅ **崩溃安全**：crash gate 验证通过
- ✅ **性能**：449K ops/sec 写入速度
- ⚠️ **架构限制**：基于 redb 三元组存储，深度遍历性能有瓶颈

### 1.2 v2 版本（开发中，架构重构）

#### 已完成（M0-M3 基础框架）

- ✅ **存储引擎（M0-M2）**：
  - Pager（8KB page）+ WAL Replay
  - IDMap + MemTable + Snapshot（Log-Structured Graph）
  - CSR Segments + 显式 Compaction
  - Durability/Checkpoint/Crash Model
  - Crash Gate 验证

- ✅ **查询框架（M3）**：
  - Parser/Lexer/AST（从 v1 复制）
  - Planner 框架（定义了 Filter/Create/Delete 等节点）
  - Executor 基础结构
  - Query API（prepare/execute_streaming）

#### 功能缺失（关键问题）

- ❌ **WHERE 子句**：Planner 定义了 `FilterNode`，但 Executor 未实现
- ❌ **CREATE 语句**：Planner 定义了 `CreateNode`，但 Executor 未实现
- ❌ **DELETE 语句**：Planner 定义了 `DeleteNode`，但 Executor 未实现
- ❌ **属性过滤**：不支持节点/关系属性比较
- ❌ **多跳查询**：仅支持单跳 `MATCH (n)-[:rel]->(m)`
- ❌ **属性访问**：RETURN 中无法返回节点属性

**当前仅支持**：
- `RETURN 1`（smoke test）
- `MATCH (n)-[:<u32>]->(m) RETURN n, m LIMIT k`（单跳，无过滤）

## 二、距离"图数据库界的 SQLite"还有多远？

### 2.1 产品定位差距

| 维度 | SQLite 标准 | v1 现状 | v2 现状 | 差距 |
|------|------------|---------|---------|------|
| **嵌入式** | ✅ 单文件，零配置 | ✅ `.redb` 单文件 | ✅ `.ndb+.wal` 双文件 | 接近 |
| **崩溃安全** | ✅ ACID 事务 | ✅ 已验证 | ✅ 已验证 | ✅ 达标 |
| **查询语言** | ✅ SQL 完整子集 | ✅ Cypher 完整子集 | ❌ 仅 RETURN 1 + 单跳 | **巨大差距** |
| **基础 CRUD** | ✅ 完整支持 | ✅ 完整支持 | ❌ 仅读，无写 | **关键缺失** |
| **性能** | ✅ 可预测 | ✅ 449K ops/sec | ⚠️ 未验证 | 待验证 |
| **多语言绑定** | ✅ C API | ✅ Node/Python/C | ⚠️ 仅 Rust | 待补齐 |

### 2.2 里程碑定义

#### v2.0.0-alpha1（当前目标）

**必须完成**：
- [ ] **CI 全绿**（含 crash-gate-v2）
- [ ] **基础查询闭环**：
  - [ ] `CREATE (n:Label {k: v})` - 创建节点和关系
  - [ ] `MATCH ... WHERE ... RETURN` - 属性过滤查询
  - [ ] `DELETE` / `DETACH DELETE` - 删除节点和关系
- [ ] **CLI 稳定**：`nervusdb v2 query` 能稳定执行上述查询
- [ ] **文档化 Cypher 子集**：明确 alpha1 支持的功能清单
- [ ] **测试覆盖**：功能符合性测试集

**当前状态**：❌ 0/5 完成

#### v2.0.0 正式版（长期目标）

**必须完成**：
- [ ] 稳定的公开 Rust API
- [ ] 基础读写闭环（CREATE/MATCH/WHERE/RETURN/LIMIT）
- [ ] 数据一致性测试（crash gate、恢复语义）
- [ ] 性能基准（对比 v1，证明架构优势）
- [ ] 多语言绑定（至少 Node.js）

**预估差距**：约 3-6 个月开发周期

## 三、Gemini 建议总结

### 3.1 产品定位建议

1. **立即调转方向**：暂停对存储引擎的过度打磨，将 80% 精力投入"最小可用 Cypher 子集"
2. **定义核心子集**：
   - `CREATE`（节点和关系，带属性）
   - `MATCH ... WHERE ... RETURN`（属性过滤）
   - `DELETE` / `DETACH DELETE`
   - `LIMIT` / `ORDER BY`
3. **管理预期**：明确告知社区这是预览版，功能有限但架构已更新

### 3.2 技术架构建议

1. **复用 v1 查询引擎**：
   - Parser/Planner 逻辑通用，可直接复用
   - 重点重写 Executor 的物理算子，对接 v2 存储
   - 使用适配器模式，将 v1 查询逻辑与 v2 存储解耦

2. **性能优势体现**：
   - 设计高效的 `Expand` 算子，利用 CSR 格式快速获取邻居
   - 在基准测试中优先设计能体现遍历优势的场景

### 3.3 工程实践建议

**v2 alpha1 关键路径**：

1. **[首要] 确定并文档化 Cypher 子集** → `docs/product/v2-cypher-subset.md`
2. **[开发] 查询解析**：扩展 Parser 支持 WHERE/CREATE（可从 v1 迁移）
3. **[开发] 查询计划**：实现 WHERE 过滤条件下推
4. **[开发] 执行器算子**：
   - `Filter` 算子：WHERE 条件过滤
   - `Create` 算子：调用 v2 存储 API 写入
   - `Delete` 算子：调用 v2 存储 API 删除
5. **[测试] 功能符合性测试集**：建立 v1/v2 对比测试
6. **[集成] CLI 升级**：支持新的查询功能
7. **[CI] CI 流水线**：确保所有测试自动运行

## 四、行动计划

### 4.1 立即行动（P0）

**⚠️ 关键发现**：v2 存储层**目前不支持属性存储**。代码审查显示：
- `EdgeKey` 只包含 `src, rel, dst`，无属性
- `MemTable`/`L0Run` 只存储边和 tombstone，无属性
- `GraphSnapshot` trait 没有属性访问方法
- WAL 记录中没有属性相关的记录类型

**这意味着必须先实现属性存储层，才能支持 WHERE 过滤和 CREATE 带属性。**

1. **实现属性存储层（关键路径）**
   - 任务：在 `nervusdb-v2-storage` 中实现属性存储
   - 需要：
     * 扩展 `MemTable` 支持节点/关系属性（HashMap<InternalNodeId, HashMap<String, Value>>）
     * 扩展 `L0Run` 包含属性数据
     * 扩展 WAL 记录类型（`SetNodeProperty`, `SetEdgeProperty`）
     * 扩展 `GraphSnapshot` trait 添加 `node_property()`, `edge_property()` 方法
   - **预估时间**：2-3 周

2. **创建 v2 Cypher 子集规范文档**
   - 文件：`docs/product/v2-cypher-subset.md`
   - 内容：明确 alpha1 支持的功能清单和使用示例

3. **实现 Filter 算子**
   - 任务：在 `nervusdb-v2-query/src/executor.rs` 中实现 `Filter` 算子
   - 依赖：属性存储层完成

4. **实现 Create 算子**
   - 任务：在 `nervusdb-v2-query/src/executor.rs` 中实现 `Create` 算子
   - 依赖：属性存储层完成

5. **实现 Delete 算子**
   - 任务：在 `nervusdb-v2-query/src/executor.rs` 中实现 `Delete` 算子
   - 依赖：v2 存储已支持 `tombstone_node/tombstone_edge`（✅ 已完成）

### 4.2 短期目标（P1，alpha1 前）

1. **属性存储支持**（如果缺失）
   - 检查 v2 存储是否支持节点/关系属性
   - 如缺失，需要实现属性存储层

2. **功能符合性测试**
   - 创建测试套件，覆盖核心子集的所有功能
   - 建立 v1/v2 对比测试（如果可能）

3. **CLI 升级**
   - 确保 `nervusdb v2 query` 支持新功能
   - 添加示例和文档

### 4.3 中期目标（P2，正式版前）

1. **性能基准**
   - 设计能体现 v2 架构优势的基准测试
   - 对比 v1 性能数据

2. **多语言绑定**
   - Node.js 绑定（优先）
   - Python 绑定

3. **文档完善**
   - API 文档
   - 迁移指南（v1 → v2）

## 五、风险评估

### 5.1 技术风险

- **属性存储缺失**：如果 v2 存储层未实现属性存储，需要额外开发周期
- **性能未验证**：v2 架构的理论优势需要通过实际测试验证

### 5.2 产品风险

- **功能倒退**：v2 功能远少于 v1，可能导致用户流失
- **预期管理**：需要明确告知社区 v2 的当前状态和路线图

### 5.3 工程风险

- **时间压力**：补齐基础功能需要 1-2 个月开发周期
- **测试覆盖**：新功能需要充分的测试保障

## 六、结论

**核心建议**：立即将开发重心从存储引擎优化转向查询功能补齐。v2 的存储架构已经完成，但查询层的功能缺失导致产品无法使用。只有补齐基础 CRUD 能力，v2 才能体现其价值。

**关键路径**：Filter → Create → Delete → 测试 → CLI → 文档 → alpha1 发布

**时间估算**：v2.0.0-alpha1 预计需要 **8-12 周开发周期**（属性存储需要开发，这是关键路径）。

**关键路径时间分解**：
- 属性存储层：2-3 周
- Filter/Create/Delete 算子：2-3 周
- 测试和集成：2-3 周
- CLI 和文档：1-2 周
- CI 和稳定性：1 周

