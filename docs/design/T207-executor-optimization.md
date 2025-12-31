# T207 Implementation Plan: Query Executor Optimization

## 1. Overview

当前查询执行器 (`nervusdb-v2-query/src/executor.rs`) 使用 `Box<dyn Iterator<Item = Result<Row>>>` 作为统一返回类型。这带来以下开销：

1. **堆分配**：每个 Plan 节点返回 boxed iterator
2. **虚函数调用**：每次 `.next()` 需要动态分发
3. **无法内联优化**：编译器无法跨 Plan 边界优化

对于高性能场景（如 >100K 节点遍历），这些开销可能成为瓶颈。

### 目标

探索并实现执行器优化策略，减少动态分发开销。

---

## 2. Requirements Analysis

### 2.1 Use Scenarios

1. 大规模图遍历查询（如社交网络 2 度关系查询）
2. 聚合查询（COUNT/SUM over 大数据集）
3. 批量数据导出

### 2.2 Constraints

- 保持 API 兼容性：`execute_plan` 签名不变
- 保持代码可维护性：不引入过度复杂的泛型
- 渐进式优化：优先优化热点路径

### 2.3 Performance Goals

- NodeScan + Filter：减少 50% 虚函数调用
- 总体查询延迟降低 10-20%（需 benchmark 验证）

---

## 3. Design Options

### Option A: Enum-based Iterator (推荐)

将所有 Plan 变体的迭代器封装为单一枚举类型，消除 boxing：

```rust
pub enum PlanIterator<'a, S: GraphSnapshot> {
    ReturnOne(std::iter::Once<Result<Row>>),
    NodeScan(NodeScanIter<'a, S>),
    Filter(FilterIter<'a, S>),
    // ... other variants
}

impl<'a, S: GraphSnapshot> Iterator for PlanIterator<'a, S> {
    type Item = Result<Row>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::ReturnOne(iter) => iter.next(),
            Self::NodeScan(iter) => iter.next(),
            // ...
        }
    }
}
```

**优点**：

- 零堆分配
- 编译器可内联 switch-case
- 保持代码结构清晰

**缺点**：

- 枚举膨胀：需为每个 Plan 类型定义 variant
- 嵌套迭代器处理复杂（如 Filter wraps NodeScan）

### Option B: Cranelift/LLVM JIT

运行时编译查询计划为机器码。

**评估**：过于复杂，不适合当前阶段。

### Option C: Pull-based Vectorized Execution

每次返回一批 Row 而非单个 Row。

**评估**：需要大幅重构 API，风险高。

---

## 4. Selected Approach: Enum Iterator (Option A)

### 4.1 Architecture

```
execute_plan(snapshot, plan, params)
    -> PlanIterator<'a, S>  // enum, not Box<dyn>

PlanIterator::NodeScan
    -> NodeScanIter { snapshot, label_id, position }

PlanIterator::Filter
    -> FilterIter { input: Box<PlanIterator>, predicate }
```

**Note**: 嵌套时仍需 boxing inner iterator，但减少了顶层分配。

### 4.2 Hybrid Strategy

对于热点路径（NodeScan, Filter, Project）使用 enum；
对于复杂/罕见路径（Aggregate, OrderBy）保留 `Box<dyn>`。

---

## 5. Implementation Plan

### Step 1: Define PlanIterator Enum (Risk: Medium)

- File: `nervusdb-v2-query/src/executor.rs`
- 定义 `PlanIterator` 枚举
- 实现 `Iterator` trait

### Step 2: Refactor NodeScan (Risk: Low)

- 将当前 closure-based 实现提取为 `NodeScanIter` struct
- 添加为 `PlanIterator::NodeScan` variant

### Step 3: Refactor Filter (Risk: Medium)

- `FilterIter` 内部持有 `Box<PlanIterator>` 或 `Box<dyn Iterator>`
- 逐步迁移

### Step 4: Benchmark (Risk: Low)

- 使用现有 benchmark 工具对比性能
- 记录结果到 `docs/perf/`

### Step 5: Cleanup (Risk: Low)

- 移除已废弃的 closure-based 实现
- 更新文档

---

## 6. Verification Plan

### 6.1 Unit Tests

```bash
cd nervusdb-v2-query && cargo test executor -- --nocapture
```

确保所有现有测试通过。

### 6.2 Integration Tests

```bash
cargo test --package nervusdb-v2 --test query_integration
```

### 6.3 Benchmark

```bash
cargo bench --package nervusdb-v2 -- query
```

或使用现有 CLI benchmark：

```bash
cargo run -p nervusdb-cli -- v2 bench --nodes 10000 --depth 2
```

### 6.4 Manual Verification

1. 运行现有查询测试套件
2. 对比优化前后的火焰图（可选）

---

## 7. Risk Assessment

| Risk Description               | Impact Level | Mitigation Measures        |
| ------------------------------ | ------------ | -------------------------- |
| 枚举膨胀导致编译时间增加       | Low          | 仅覆盖热点路径             |
| 嵌套迭代器 boxing 削弱优化效果 | Medium       | 保守估计收益               |
| API 变更导致下游代码不兼容     | Low          | 保持 execute_plan 签名不变 |

---

## 8. Out of Scope

- JIT 编译
- Vectorized execution
- 并发执行（当前模型为单线程迭代）

---

## 9. Future Extensions

- **Arena Allocation**：使用 arena 分配 Row，减少 heap 压力
- **Fused Operators**：合并 Filter + Project 为单一 operator
- **SIMD Filtering**：对数值属性使用 SIMD 比较
