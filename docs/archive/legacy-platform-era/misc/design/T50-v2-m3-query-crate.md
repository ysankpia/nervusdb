# T50: v2 M3 — Query Crate（复用 Parser/Planner 的落地）

## 1. Context

v2（`nervusdb-storage`）已经提供 `nervusdb-api::{GraphStore, GraphSnapshot}` 的 streaming 边界（见 `docs/design/T47-v2-query-storage-boundary.md`）。目前缺口在查询层：Cypher 文本 → AST/Plan → 基于 `GraphSnapshot` 的 executor。

v1 的 `nervusdb-core/src/query` 里 parser/AST/planner 质量不错，但 executor 深度绑定 `redb`，无法直接复用。

## 2. Goals

- 在 workspace 新增 crate：`nervusdb-query`
- **复用策略务实**：先“复制” v1 的 `query/{ast,lexer,parser,planner}` 到 v2-query（避免早期抽共享导致两边互相拖死）
- 定义 v2-query 的公开入口（先不承诺完整 openCypher）：`parse_cypher()` / `plan()` 的最小 API
- 明确 M3 的 Cypher 最小子集（为 T51/T52 的 executor/API 做输入边界）

## 3. Non-Goals

- 不做 v1/v2 的共享 query crate（后续再考虑抽象）
- 不在 T50 实现 executor（由 T51 负责）
- 不承诺 80% openCypher 覆盖（M3 只做可跑通的最小子集）

## 4. Proposed Solution

### 4.1 Workspace / Crate

- 新增 `nervusdb-query/`
  - `src/ast.rs` / `src/lexer.rs` / `src/parser.rs` / `src/planner.rs`
  - `src/lib.rs` 统一 re-export，并提供稳定入口函数

Cargo 依赖原则：

- 只允许小而确定的 crates（如 `thiserror`），禁止引入大型运行时
- v2-query **不得**依赖 `nervusdb-storage`，只能依赖 `nervusdb-api`（通过 trait 交互）

### 4.2 Minimal Cypher Subset (M3)

仅承诺下面这几个“骨架能力”，其他语法直接 `NotSupported`（不要靠 if/else 补洞）：

- `CREATE (n:Label {k: v, ...}) RETURN n`
- `MATCH (n:Label)-[r:REL]->(m:Label) WHERE <simple predicate> RETURN <projection> [LIMIT n]`
- `RETURN 1`（用于 smoke）

谓词（M3）只支持：

- `=` / `<>` / `>` / `>=` / `<` / `<=`
- `AND` / `OR`
- 字面量：整数/浮点/字符串/bool/null

### 4.3 Planner Output Boundary (for T51)

T50 只需要保证 planner 的输出能够驱动一个极简 executor：

- `ScanLabel(LabelId)`（如暂时没有 label scan，则 planner 只允许从 pattern 的起点变量开始做 expand）
- `Expand(src_var, rel_type, dst_var, direction=out)` → 映射为 `GraphSnapshot::neighbors(src_iid, Some(rel))`
- `Filter(expr)`
- `Project(columns)`
- `Limit(n)`

具体 plan 结构可以沿用 v1 的 `LogicalPlan/PhysicalPlan` 形状，但必须切断对 v1 storage 的任何引用。

## 5. Testing Strategy

- `nervusdb-query` 单元测试：
  - parser：最小子集的正例与 NotSupported 的反例
  - planner：把 AST 转成 plan 的结构测试（不触发存储）
- 不引入快照 golden 文件（先用结构断言，避免早期测试变成维护负担）

## 6. Risks

- v1 parser/planner 复制后可能带来大量“不必要的语法分支”，必须在 v2-query 入口处强制做子集 gate（否则 executor 会被迫支持一堆边角）
- planner 若继续输出 v1 的执行节点，会把 executor 绑死在旧模型；必须尽早改造为面向 `GraphSnapshot` 的算子集合

