# T322: Multi-Label Model + SET/REMOVE Labels Implementation Plan

## 1. 概述

### 目标

支持 Cypher 标准的多标签模型，允许节点拥有多个标签，并支持 `SET` 和 `REMOVE` 动态修改标签。

### 优先级

**High** - 这是 Cypher 合规性的核心功能

---

## 2. 需求分析

### 2.1 当前限制

**单标签模型**：

- 每个节点只能有一个 `LabelId`
- `create_node(external_id, label_id)` 接受单个标签
- `IdMap` 存储 `Vec<LabelId>`（每节点一个）

**问题**：

- ❌ 不支持 `CREATE (n:Person:Employee)`
- ❌ 不支持 `SET n:Manager`
- ❌ 不支持 `REMOVE n:Employee`
- ❌ 不符合 Cypher 标准

### 2.2 功能需求

#### 必须支持（Must-Have）

- [ ] 节点创建时支持多标签：`CREATE (n:A:B:C)`
- [ ] 动态添加标签：`SET n:NewLabel`
- [ ] 动态删除标签：`REMOVE n:OldLabel`
- [ ] 模式匹配多标签：`MATCH (n:Person:Employee)`
- [ ] 标签存在性检查优化

#### 应该支持（Should-Have）

- [ ] `labels(n)` 函数返回标签列表
- [ ] 索引支持多标签（`CREATE INDEX ON :Person(name)`）
- [ ] 统计信息按标签分组

#### 可选（Nice-to-Have）

- [ ] 标签继承/层次结构（非 Cypher 标准）

---

## 3. 设计方案

### 3.1 存储层变更

#### IdMap 结构调整

**当前**：

```rust
pub struct I2eRecord {
    pub external_id: ExternalId,
    pub label_id: LabelId,  // 单标签
}

pub struct IdMap {
    i2l: Vec<LabelId>,  // InternalNodeId → LabelId
}
```

**提案**：

```rust
pub struct I2eRecord {
    pub external_id: ExternalId,
    pub label_ids: Vec<LabelId>,  // 多标签（排序，去重）
}

pub struct IdMap {
    i2l: Vec<Vec<LabelId>>,  // InternalNodeId → Vec<LabelId>
}
```

**优化**：使用位图（BitSet）或压缩数据结构减少内存（可选）

#### WAL 记录扩展

新增操作：

```rust
pub enum WalRecord {
    // 现有
    CreateNode { external_id, label_id, internal_id },

    // 新增
    CreateNodeMultiLabel {  // 创建时多标签
        external_id: ExternalId,
        label_ids: Vec<LabelId>,
        internal_id: InternalNodeId,
    },
    SetLabels {  // 添加标签
        node: InternalNodeId,
        labels: Vec<LabelId>,
    },
    RemoveLabels {  // 删除标签
        node: InternalNodeId,
        labels: Vec<LabelId>,
    },
}
```

### 3.2 API 层变更

#### GraphSnapshot 扩展

**当前**：

```rust
fn node_label(&self, iid: InternalNodeId) -> Option<LabelId>;
```

**提案**：

```rust
fn node_labels(&self, iid: InternalNodeId) -> Option<Vec<LabelId>>;

// 向后兼容（返回第一个标签）
#[deprecated]
fn node_label(&self, iid: InternalNodeId) -> Option<LabelId> {
    self.node_labels(iid).and_then(|labels| labels.first().copied())
}
```

#### WriteTxn 扩展

```rust
impl WriteTxn<'_> {
    // 新增方法
    pub fn set_labels(&mut self, node: InternalNodeId, labels: Vec<LabelId>);
    pub fn add_label(&mut self, node: InternalNodeId, label: LabelId);
    pub fn remove_label(&mut self, node: InternalNodeId, label: LabelId);

    // 兼容现有 API
    pub fn create_node(&mut self, external_id: ExternalId, label_id: LabelId) -> Result<InternalNodeId>;

    // 新增多标签创建
    pub fn create_node_multi_label(&mut self, external_id: ExternalId, labels: Vec<LabelId>) -> Result<InternalNodeId>;
}
```

### 3.3 查询层变更

#### Parser 支持

**节点模式多标签**：

```cypher
MATCH (n:Person:Employee)  // 解析为 labels = ["Person", "Employee"]
CREATE (n:A:B:C RETURN n
```

**SET/REMOVE 语句**：

```cypher
SET n:Manager              // 添加单个标签
SET n:A:B                  // 添加多个标签
REMOVE n:Employee          // 删除单个标签
```

#### Executor 实现

**Plan 枚举扩展**：

```rust
pub enum Plan {
    // 现有
    MatchNode { ... },

    // 新增
    SetLabels {
        input: Box<Plan>,
        node_var: String,
        labels: Vec<String>,  // 标签名
    },
    RemoveLabels {
        input: Box<Plan>,
        node_var: String,
        labels: Vec<String>,
    },
}
```

**标签匹配语义**：

- `MATCH (n:A:B)` → 节点**必须同时拥有** A 和 B 标签（AND 语义）
- 优化：使用索引快速过滤候选节点

---

## 4. 实施步骤

### Phase 1: 存储层基础（高风险）

1. **IdMap 重构**
   - 修改 `I2eRecord` 为多标签
   - 更新 `get_label` → `get_labels`
   - 适配所有调用点
2. **WAL 记录扩展**

   - 添加 `SetLabels` / `RemoveLabels`
   - 实现序列化/反序列化
   - WAL 回放逻辑

3. **API 更新**
   - `node_label` → `node_labels`
   - `WriteTxn` 新增标签操作方法

### Phase 2: 查询层支持（中风险）

1. **Parser 扩展**
   - 支持多标签模式 `:A:B:C`
   - 解析 `SET`/`REMOVE` 语句
2. **Executor 实现**

   - `Plan::SetLabels` / `Plan::RemoveLabels`
   - 标签过滤语义（AND）

3. **内置函数**
   - `labels(n)` 返回标签列表

### Phase 3: 优化与测试（中风险）

1. **索引适配**

   - 多标签索引维护
   - 标签变更时更新索引

2. **统计信息**

   - 按标签组合统计节点数

3. **集成测试**
   - 多标签创建/匹配
   - SET/REMOVE 操作
   - 边界情况（空标签、重复标签）

---

## 5. 风险评估

| 风险                 | 影响 | 缓解措施                     |
| -------------------- | ---- | ---------------------------- |
| **IdMap 破坏性变更** | 高   | 保留向后兼容 API，渐进迁移   |
| **WAL 格式不兼容**   | 高   | 版本号升级，迁移脚本         |
| **性能回归**         | 中   | 标签数量限制（≤8），使用位图 |
| **索引失效**         | 中   | 测试覆盖多标签索引场景       |

---

## 6. 验证计划

### 6.1 单元测试

- IdMap 多标签 CRUD
- WAL 序列化/回放
- 标签操作（SET/REMOVE）

### 6.2 集成测试

```cypher
// Test Case 1: 多标签创建
CREATE (alice:Person:Employee {name: "Alice"})
MATCH (n:Person:Employee) RETURN count(n)  // 应返回 1

// Test Case 2: SET 标签
CREATE (bob:Person {name: "Bob"})
MATCH (n:Person {name: "Bob"}) SET n:Manager
MATCH (n:Person:Manager) RETURN n.name  // 应返回 "Bob"

// Test Case 3: REMOVE 标签
CREATE (charlie:Person:Employee {name: "Charlie"})
MATCH (n {name: "Charlie"}) REMOVE n:Employee
MATCH (n:Employee {name: "Charlie"}) RETURN count(n)  // 应返回 0
```

### 6.3 性能测试

- 100 万节点，平均 3 个标签
- 标签匹配查询性能（有/无索引）

---

## 7. 向后兼容性

**策略**：

1. 保留 `create_node(eid, label_id)` API（内部转为 `vec![label_id]`）
2. `node_label()` 废弃但保留（返回第一个标签）
3. WAL 向后兼容（区分 v1/v2 记录）
4. 迁移工具：将旧数据库单标签转为多标签格式

---

## 8. 需要用户确认

> [!IMPORTANT] > **破坏性变更**：
>
> - IdMap 存储格式变更
> - WAL 格式变更
> - API 签名变更（`node_label` → `node_labels`）
>
> 这些变更需要版本升级和数据迁移。

**问题**：

1. 是否接受破坏性变更？（建议：v2.1 → v2.2）
2. 标签数量是否设限制？（建议：≤8 个标签/节点）
3. 是否需要标签位图优化？（可后续迭代）

---

## 9. 参考

- Neo4j 多标签实现：https://neo4j.com/docs/cypher-manual/current/syntax/expressions/#syntax-label-expressions
- openCypher 规范：https://opencypher.org/
