# T51: v2 M3 — Executor MVP（基于 GraphSnapshot 的流式算子）

## 1. Context

v2 已经具备可验证的存储内核（Pager/WAL/Snapshot/L0Runs/CSR segments）。查询层缺口在 executor：必须把“计划节点”映射到 `nervusdb-api::GraphSnapshot` 的 streaming 访问，而不是把结果 collect 到内存。

当前 `GraphSnapshot` 只有 `neighbors()`，没有 node scan / label / id 映射能力，因此无法实现最基本的 `MATCH (n)`。

## 2. Goals

- 实现一个 **Pull-based** 的 streaming executor（iterator pipeline）
- 支持最小可用图查询（MVP），以便验证 v2 的读路径与执行器边界
- **不破坏现有 v2 内核**：只允许在 `nervusdb-api` 增加“向后兼容”的扩展（默认实现），并在 `nervusdb-storage` 里补实现

## 3. Non-Goals

- 不做并行 morsel / 向量化（后置到 M4）
- 不做复杂 join reorder / cost-based（planner 优化后置）
- 不做属性读取/过滤（等 `GraphSnapshot::get_props()` 契约明确后再做）

## 4. API Boundary Updates（必要时）

为支撑 `MATCH (n)` 与 label 过滤，允许在 `nervusdb-api` 追加以下接口（都提供默认实现，保证向后兼容）：

```text
trait GraphSnapshot {
  type Nodes<'a>: Iterator<Item = InternalNodeId> + 'a;
  fn nodes(&self) -> Self::Nodes<'_>; // full scan (MVP)

  fn resolve_external(&self, iid: InternalNodeId) -> Option<ExternalId>;
  fn node_label(&self, iid: InternalNodeId) -> Option<LabelId>;
}
```

实现策略（storage）：

- nodes(): 基于 IDMap 的 I2E 长度做 0..len 扫描（过滤 tombstone 在后续做）
- node_label/resolve_external(): 从 I2E record 读取（M1 阶段用数组页；无需 B+Tree）

备注：如果 tombstone 需要在 scan 阶段过滤，可以在 snapshot 内部提供 `is_tombstoned_node(iid)`（同样 default）。

## 5. Executor MVP：最小算子集

执行器采用“行管线”，每个算子都是一个 `Iterator<Item = Row>`：

- `NodeScan`：产生绑定变量（例如 `n`）的 InternalNodeId
- `ExpandOut`：对输入行中 `src` 做 `neighbors(src, rel)`，输出扩展后的行（绑定 `m` / `r`）
- `Filter`：只支持与拓扑相关的谓词（MVP）：
  - label 过滤（`node_label(iid) == label_id`）
  - rel type 过滤（`neighbors(src, Some(rel))` 已下推）
  - id equality（外部 id 仅做展示，不做 where）
- `Project`：返回列（MVP：返回 internal id / external id）
- `Limit`

Row 表示（MVP）：

- `Row` = 变量名 → `Value`（只需要 `NodeId(InternalNodeId)` / `EdgeKey` / `Int/String/Null`）

## 6. Testing Strategy

- `nervusdb-storage` + `nervusdb-query` 的集成测试：
  - 构造小图（3-5 节点，几条边），执行 `MATCH` 单跳，断言返回行数与内容
  - 覆盖 snapshot isolation：读快照在写提交后仍稳定（可复用现有 v2 tests 模式）
- executor 单测：算子组合（Scan → Expand → Limit）的行为测试（不依赖解析器）

## 7. Risks

- 如果 API 扩展（nodes/label）不做，executor 会被迫引入“要求用户提供起点”的奇怪语义，这会把 Cypher 兼容性带偏
- node scan + tombstone 过滤如果实现粗糙，可能导致 iterator 逻辑复杂化；必须尽量把过滤逻辑收敛在 snapshot 层，避免 executor 到处写 if

