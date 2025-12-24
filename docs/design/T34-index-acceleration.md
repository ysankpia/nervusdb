# T34: FTS Index Pushdown (txt_score)

## 1. Context

T33 已经把 `txt_score(n.prop, $q)` / `vec_similarity(n.prop, $v)` 落地成“函数式评分”，并提供了 FTS/Vector sidecar。

现实问题也很直接：**评分函数再快，只要执行器还在扫海量节点，吞吐就还是垃圾。**  
Phase 2 的目标就是让“索引产出候选集”，而不是让“全表产出候选集再算分”。

## 2. Goals

- 对最常见的检索形态做下推加速（不改 Cypher 语法）：
  - `WHERE txt_score(n.prop, $q) > t`
- 保证 **不破坏用户可观察语义**（尤其是结果集与过滤条件的一致性）。
- 实现要“蠢但清晰”：尽量少引入 planner special cases，避免把 executor 搞成一坨 if/else。

## 3. Non-Goals

- 不新增 Cypher 语法糖（例如 `MATCH ... WHERE n.prop ~ $q`）。
- 不做通用代价模型/全局重排（先做单点识别）。
- 不搞后台异步索引 commit / 自动 flush（仍由 `flush_indexes()` 控制）。
- 不做 Vector 下推；Vector 走 Top-K（Sort+Limit）下推单独放到 T35。

## 4. Key Insight (Why FTS pushdown is safe)

FTS 的 `txt_score` 语义本来就是 **TopK 截断**（见 `TXT_SCORE_TOP_K`），因此当 predicate 为 `> 0`（或 `>= 正数`）时，
用索引先产出候选集再做过滤，与“全量扫描每行调用 `txt_score`”语义一致。

## 5. Solution

### 5.1 Data Flow

核心数据关系很简单：

- 输入：`(property, query, limit/threshold)`  
- 输出：候选 `(node_id, score)` 列表（规模远小于全量节点）

然后把 `(node_id)` 绑定到 planner 的 `Scan(alias)` 结果，后续 Expand/Join/Project 都照旧跑。

### 5.2 Planner Pattern Matching (FTS first)

第一阶段只做最小可控的识别：

- 只处理形态：`Filter(predicate, input=Scan(alias=n, labels=[...]))`
- predicate 中存在顶层 AND 里的子表达式：
  - `txt_score(n.<prop>, <query>) > <threshold>`
  - `<query>` 只能是参数或字符串字面量（不能依赖 record 的变量）

命中后，把 `ScanNode` 替换为 `FtsCandidateScanNode`（新 PhysicalPlan variant）：

- 执行时用 tantivy 搜索一次拿到 TopK 的 `node_id -> score`
- 若 `ScanNode.labels` 非空，则在候选集上做 label 过滤（候选集小，允许 O(K * label_check)）
- 输出 record：`{ n: Node(node_id) }`

FilterNode 仍保留：

- 对 threshold 与其它 predicate 继续生效
- `txt_score(...)` 会命中 cache（O(1) map lookup），不再触发 per-row 搜索

### 5.3 Optional Follow-ups

- 若 query 同时有 `ORDER BY txt_score(...) DESC LIMIT k`，可以让候选 scan 按 score 产出并前推 Limit，避免 Sort materialize。

## 6. Testing Strategy

- Planner 单测：给定 `Filter(Scan)` + `txt_score` predicate，验证重写为 `FtsCandidateScan`。
- Executor/Integration（feature `fts`）：
  - 小数据集：命中/不命中、label 过滤、threshold 过滤一致
  - `flush_indexes()` 前后可见性不变（仍按 T33 规则）
- 兼容性：未启用 `fts` feature 或未配置索引时，planner 不做下推（保持旧行为）。

## 7. Risks

- Planner 识别范围越大，越容易引入隐式语义差异；必须从最窄可验证的 pattern 开始。
- `txt_score(...) >= 0` 不能下推（会把“全量都满足”的语义错误压缩到 TopK）；必须显式规避。
