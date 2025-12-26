# T47: v2 Query ↔ Storage 边界（Executor 重写的契约）

## 1. Context

v1 query/executor 深度绑定 `redb` 迭代器；v2 采用 Snapshot + L0Runs + CSR segments。要复用 v1 的 AST/Planner，必须先把 query 与 storage 的边界写死，否则 executor 会继续“偷看底层结构”。

## 2. Goals

- 定义 storage trait（query 只依赖 trait）
- 定义 iterator/row 的 streaming 契约（避免 collect）
- 明确最小算子集（MATCH expand + filter + return）如何落地

## 3. Non-Goals

- 不在 T47 定完整优化器/并行 morsel 模型（后置）
- 不在 T47 引入二级索引选择（M3+）

## 4. Storage Trait（MVP）

```text
trait GraphStore {
  type Snapshot: GraphSnapshot;
  fn snapshot(&self) -> Self::Snapshot;
}

trait GraphSnapshot {
  fn neighbors(&self, src: InternalNodeId, rel: Option<RelTypeId>) -> EdgeIter;
  fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId>; // M1 via I2E
  // 后续：
  // fn scan_label(label) -> NodeIter
  // fn get_props(...)
}
```

## 5. Executor 契约

- executor 必须只消费 `GraphSnapshot`，不得触碰 WAL/pager/segments
- 返回必须是 streaming iterator（不得把全结果 collect 到 Vec）

## 6. 复用策略（务实）

- v2 query crate（未来）先“复制” v1 的 parser/AST/planner（避免早期抽共享）
- executor 重写：把核心算子映射到 `neighbors/scan/filters` 等接口

