# T35: Vector Top-K Pushdown (ORDER BY + LIMIT)

## 1. Context

T33 提供了：

- `vec_similarity(n.prop, $query_vec)`（精确 cosine 相似度函数）
- 可选向量索引 sidecar（`usearch` / HNSW）与 `Database::vector_search(query, k)`

但现在执行器还是按“先扫行、每行算分、再 sort、再 limit”的套路跑。对向量检索这是典型的性能自杀。

与此同时，**不要把 `WHERE vec_similarity(...) > t` 下推到 HNSW**：那是 range search（r-NN）语义，不是 k-NN，结果会漏，属于隐式语义破坏。

所以 T35 的目标很明确：只做 **Top-K**。

## 2. Goals

- 针对模式进行加速（不改 Cypher 语法）：

```cypher
MATCH (n)
ORDER BY vec_similarity(n.vec, $q) DESC
LIMIT k
```

- 优先做最小识别：单个 `ORDER BY` 项且为 `vec_similarity(...) DESC`，并且有 `LIMIT k`。
- 保持实现简单：planner 做结构重写，executor 走 `usearch.search(query, k)` 拿候选集。

## 3. Non-Goals

- 不做 `WHERE vec_similarity(...) > t` 下推（range search 语义不成立）。
- 不做 label 子集 Top-K（除非后续引入 label-specific index 或明确定义近似策略）。
- 不做多列排序 / SKIP / 复杂管线重排（先把最常见的“向量 Top-K”吞吐提上去）。

## 4. Solution

### 4.1 Planner Pattern

识别最终 plan 形态（概念上）：

```
Limit(
  Sort(
    Project(
      Scan(alias=n, labels=[]),
      ...
    ),
    order_by=[vec_similarity(n.<prop>, $q) DESC]
  ),
  limit=k
)
```

重写为：

```
Project(
  VectorTopKScan(alias=n, property=<prop>, query=$q, k=k),
  ...
)
```

其中 `VectorTopKScan` 在运行时：

- 若 vector feature 未启用 / 索引未配置 / 配置不匹配 → 回退到原始 Scan+Sort+Limit 逻辑（保证不因为没索引就变成空结果）。
- 若索引可用 → `usearch.search(query, k)` 直接返回候选 `node_id` 列表（按相似度降序）。

### 4.2 Gating（必须有护栏）

为了避免偷改语义或返回垃圾数据，下推至少需要：

- `Scan.labels` 为空（只做全局 Top-K）。
- `vec_similarity` 左参数必须是 `PropertyAccess` 且变量名与 Scan alias 相同。
- query 参数必须能解析为 `Vector`（通常是 `$param`）。
- 索引配置的 property 与函数访问的 property 一致。
- 索引 metric 必须是 cosine（否则 order-by 的排序语义不匹配）。

## 5. Testing Strategy

- Planner 单测：识别 `ORDER BY vec_similarity(...) DESC LIMIT k` 时发生重写；其它情况不重写。
- Integration（feature `vector`）：
  - 配置向量索引，写入向量属性，验证查询返回 `k` 条且顺序符合“相似度降序”。
  - 未配置索引时仍能返回非空结果（走 fallback）。

## 6. Risks

- Top-K 下推仍是 ANN：结果是近似的，但语义与 k-NN 对齐；这比 range 下推要干净得多。
- executor 现有 `Scan` 的“全节点发现”本身就有限制（采样），fallback 需要尽量复用现有逻辑避免引入新差异。

