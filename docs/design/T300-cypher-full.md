# T300 Implementation Plan: Full Cypher Compatibility (openCypher + NervusDB Extensions)

> **Risk**: High（语言前端 + Planner/Executor + Storage API 可能都要动）  
> **Status**: Plan（等待确认后进入分阶段实现）  
> **Owner**: Team  
> **Related**: `docs/spec.md` / `docs/tasks.md` / `docs/reference/cypher_support.md`

## 1. Overview

当前 v2 的查询引擎（`nervusdb-query`）已经具备一套可工作的 M3 子集，但整体仍是“白名单子集 + fail-fast”的定位。  
本任务的目标是把“Cypher 兼容”从口号变成**可验收的工程契约**：明确我们要兼容哪一版 Cypher、哪些不做（或做成扩展），并用自动化测试（优先 TCK）作为发布门禁。

> 重要：这里的“Full Cypher”不是一句话的许愿，而是一组**可执行的验收标准**（tests as contract）。没有这一步，任何“全量支持”的说法都会变成文档诈骗。

## 2. Requirements Analysis

### 2.1 Use Scenarios

1. **应用侧复杂查询**：多阶段管道（`WITH`）+ 聚合/过滤 + 多模式组合（join/optional）+ 分页排序。
2. **数据处理/ETL**：`UNWIND` 展开列表进行批量写入/更新，或做集合运算（`UNION`）。
3. **子查询与扩展能力**：`CALL { ... }` 子查询；以及 NervusDB 扩展（例如向量检索）的 `CALL vector.search(...)`。

### 2.2 Functional Requirements

#### Must（发布阻塞）

- **兼容目标可声明**：明确“兼容 openCypher 的哪个版本/commit（或 Cypher 版本号）”，并给出排除项清单。
- **TCK/门禁**：引入可重复运行的兼容性测试集（优先 openCypher TCK；若无法全量引入，至少要有“固定场景黄金用例 + 可扩展 harness”）。
- **语法与语义一致**：不再依赖“能解析但执行时报 not implemented”来冒充支持；发布口径必须以门禁通过为准。

#### Should（强烈建议）

- **分阶段可交付**：每个阶段都能跑测试、可回滚，不搞“半年大爆炸”。
- **对外接口稳定**：CLI/Python binding 的行为变化要可控（必要时版本化或提供兼容层）。

### 2.3 Performance Goals

- 在不牺牲 crash-safety/一致性的前提下，避免引入显著的性能回退。
- 对“常见查询路径”（scan/expand/filter/aggregate/order/limit）提供最基本的性能基线与回归测试（可先用 microbench + 行为测试）。

## 3. Design

### 3.1 Compatibility Contract（先把“全量”定义清楚）

建议把“Full Cypher”拆成三层契约（逐层达标）：

1. **Parse-Complete（语法全量）**：能解析目标版本 openCypher 的语法并生成 AST（不等于可执行）。
2. **Plan-Complete（规划全量）**：AST 能编译为 Logical Plan / Physical Plan（仍可能缺执行器细节）。
3. **Exec-Complete（语义全量）**：通过约定的 TCK/黄金用例，且文档对外承诺与实际一致。

> 发布口径默认以 **Exec-Complete** 为准；除非明确标注“仅语法支持”。

### 3.2 Architecture Decisions

#### A. Parser Strategy

现状 parser 是手写递归下降（适合小子集）。要走向 full grammar，有两个方向：

- **方案 A（延续手写）**：持续扩展现有 `lexer.rs/parser.rs`。优点是依赖少、改动可控；缺点是 grammar 复杂度迅速失控（优先级、回溯、歧义、错误恢复）。
- **方案 B（引入 grammar-driven parser）**：用 Rust 生态的 parser generator（如 `pest`/`lalrpop`/ANTLR 方案）实现 openCypher grammar，手写仅保留轻量的 glue。

本设计建议：
- **短期（T301-T310）**：延续手写 parser，补齐表达式与少量 clause（成本最低）。
- **中期（T311+）**：评估并切换到 grammar-driven parser，避免长期维护成本爆炸。

#### B. Planner/Executor Strategy

要支持 `WITH/UNWIND/UNION/CALL` 等，需要引入更通用的 plan 节点：

- `Project`（支持表达式，不仅是变量投影）
- `Join` / `LeftJoin`（`OPTIONAL MATCH`/多 pattern parts）
- `Unwind`（一行变多行）
- `Union` / `UnionAll`
- `Apply` / `Subquery`（`CALL { ... }`）
- `Aggregate`（完善 group-by/聚合语义）
- `Sort` / `Skip` / `Limit`（已具备雏形，但需作用域/表达式支持）

设计原则：
- 先做 **Logical Plan（语义正确）**，再逐步做优化（CBO/索引选择等）。
- 每加一个 plan 节点，都必须配套集成测试（或 TCK 子集）。

#### C. Storage/API 约束（这是 full Cypher 的“地基”）

如果目标包含 openCypher 常规语义，现有 Storage API 存在结构性缺口：

- 方向：目前仅有 outgoing neighbors，`<-`/无向匹配需要 incoming 或等价能力。
- 标签：目前节点是单 label；openCypher 允许多 label，且支持 `SET/REMOVE` label。
- 值模型：TCK 往往需要 node/relationship/path 作为一等值（不仅仅是 ID）。

因此 full Cypher 计划必须包含：
- **GraphSnapshot API 扩展**（至少补 incoming/undirected 所需能力）
- **多 label 的存储与查询语义**（含写入与索引/统计）
- **Value/Row 输出模型升级**（对 binding/CLI 是破坏性高风险点）

### 3.3 API Design（草案）

以下为方向性草案，具体以对应子任务设计为准：

- `nervusdb-api::GraphSnapshot`
  - 增加 incoming 遍历接口（或统一的 `neighbors(src, dir, rel)`）
  - 增加多 label 查询接口（例如 `node_labels(iid) -> &[LabelId]` 或迭代器）
  - 增加取 node/edge 结构化信息的方法（用于返回值与 TCK）

- `nervusdb-query`
  - `Expression`：补齐优先级、算术/字符串/IN/CASE/EXISTS 等
  - `Plan`：引入 `Join/Unwind/Union/SubqueryApply/...`
  - `Executor`：实现对应算子与类型系统

## 4. Implementation Plan

> 任务 ID 以 `docs/tasks.md` 为准；此处给出分阶段拆解与验收方式。

### Step 0: 锁定契约与门禁（Risk: High）

- Task: `T300`（本设计文档 + 兼容目标 + 测试门禁方案）
- 输出：
  - 兼容目标（openCypher 版本/commit）
  - 排除项清单（必须写清楚）
  - TCK/harness 方案（如何集成到 CI，跑哪些用例，如何扩展）
- 验收：
  - PR 级别：至少能跑“解析门禁”（parse-only）与“最小执行门禁”（当前子集）

### Step 1: 表达式与基础操作符（Risk: Medium）

- Tasks: `T301`~`T304`
  - 算术（`+ - * / % ^`）
  - 字符串操作（`STARTS WITH / ENDS WITH / CONTAINS`）
  - `IN`
  - `REMOVE`（属性移除）
- 验收：
  - 新增单测 + 集成测试覆盖核心语义
  - 与现有 `WHERE`/聚合/索引优化不冲突

### Step 2: 管道与集合（Risk: High）

- Tasks: `T305`~`T307`
  - `WITH`（含 where/order/skip/limit 语义与作用域）
  - `UNWIND`
  - `UNION/UNION ALL`
- 验收：
  - 具备多阶段查询的端到端用例
  - 明确作用域规则（变量可见性/重命名/聚合）

### Step 3: 语义补齐（Risk: Medium）

- Tasks: `T308`~`T309`
  - `CASE`
  - `EXISTS`（pattern/subquery）
- 验收：
  - 针对边界条件（NULL、类型不匹配、空集合）有稳定行为

### Step 4: 文档对齐（Risk: Medium）

- Task: `T310`
  - 以“门禁为准”更新 `docs/reference/cypher_support.md`
  - 明确哪些是扩展（例如 vector.search）

### Step 5+: Full Cypher 地基（Risk: High，预计需要新增任务）

当 Phase 4 的基础能力做完后，若目标仍是 openCypher 级别的 full semantics，必须继续推进：

- Parser 体系升级（grammar-driven）
- Pattern matching 的泛化（多 hop、方向、type alternation、路径值）
- Storage API（incoming、多 label）与 Value 模型升级
- TCK 执行门禁逐步扩大直至目标通过

> 这部分需要在 `docs/tasks.md` 中继续拆出 `T311+` 的高风险子任务，并按“先设计、再确认、再执行”的流程推进。

## 5. Technical Key Points

- **作用域/变量绑定**：`WITH`/`CALL` 会改变变量生命周期，是 planner 的核心复杂点。
- **Value 类型系统**：算术/字符串/列表/map/NULL 传播规则需要清晰且一致。
- **实体值（Node/Rel/Path）**：如果要对齐 TCK，单纯 ID 可能不够；会牵动 binding 输出。
- **性能与可回滚**：优先保证语义正确，再做优化；每一步要能回滚到稳定子集。

## 6. Verification Plan

### 6.1 Unit Tests

- expression evaluator：算术、字符串、IN、CASE、NULL 语义
- parser：优先级/结合性/错误提示（逐步扩展）

### 6.2 Integration / E2E

- `nervusdb/tests/*`：新增覆盖 WITH/UNWIND/UNION 等端到端用例
- CLI/Python binding：至少一组“用户视角黄金用例”防回归

### 6.3 Compatibility Gate（建议）

- 引入 openCypher TCK（或可扩展 harness）
  - 第一阶段：parse-only gate（确保语法覆盖）
  - 第二阶段：exec gate（逐步扩大用例集）

## 7. Risk Assessment

| Risk Description | Impact Level | Mitigation Measures |
| --- | --- | --- |
| Storage API 不支持 incoming/multi-label，导致语义无法对齐 | High | 先明确兼容目标与排除项；必要时拆出 Storage 高风险子任务 |
| parser 持续手写导致维护成本爆炸 | High | 中期切换 grammar-driven parser；提前做 PoC |
| Value/输出模型升级破坏 binding/CLI 兼容 | High | 版本化输出或提供兼容层；测试覆盖 CLI/Python |
| “语法支持”与“语义支持”口径混淆 | High | 文档强制以门禁为准；cypher_support.md 只写已实现 |

## 8. Out of Scope（当前不承诺）

- Neo4j DBMS 管理命令、权限、集群特性
- 未明确纳入兼容目标的 procedures（除非作为 NervusDB 扩展单独设计）
- 性能达到 Neo4j 级别（先保证正确性与可用性）

## 9. Future Extensions

- 向量检索：`CALL vector.search(...)`（作为扩展 procedure 体系的一部分）
- 更强的优化器：规则优化 + CBO 深化
- 兼容更多 Cypher 变体（如 Neo4j 新版本语义差异）

