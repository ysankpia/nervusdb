# T53: v2 M3 — Query Tests + CLI 验收路径

## 1. Context

没有“用户视角”的验收路径，query/executor 很容易在重构中悄悄变味。v1 已经证明：CI 里跑得过的最小 query 测试集 + CLI 的 NDJSON 输出，是最有效的防回归手段之一。

v2 M3 的目标不是把 Cypher 写完，而是把 **“最小子集 + 稳定行为”** 锁死。

## 2. Goals

- 建立 v2-query 的 **集成测试套件**（黑盒行为锁定）
- 提供一个最小 CLI 验收路径（优先复用现有 `nervusdb-cli`）：
  - 能打开 `.ndb/.wal`
  - 执行最小子集 Cypher
  - 流式输出 NDJSON（每行一个 Row）

## 3. Non-Goals

- 不追求漂亮的 CLI UX（先能用、可自动化）
- 不做 benchmark（已有 v2 bench/perf gate：T48）

## 4. Proposed Tests

### 4.1 Fixture

- 使用 `tempfile::tempdir()` 创建独立的 `.ndb/.wal`
- 用 `nervusdb` 写事务构造小图（固定 external ids/label/rel ids）

### 4.2 Test Cases（M3 最小子集）

- `RETURN 1`（smoke，验证 parser→planner→executor 全链路）
- `MATCH (n)-[:REL]->(m) RETURN n,m LIMIT 10`
- label 过滤（如果 T51 增加 `node_label`）：
  - `MATCH (n:Person)-[:KNOWS]->(m) RETURN m`
- tombstone 可见性（如果 query 暴露 nodes scan）：
  - tombstone node 后，scan 不应再产出该 node

所有测试都以 **结果行集合** 或 **行数** 断言即可，避免早期引入重型 golden 机制。

## 5. CLI 验收路径

建议在 `nervusdb-cli` 增加子命令（或新二进制）：

```text
nervusdb v2 query --db path/to/db.ndb --cypher \"MATCH ...\" --ndjson
```

输出规则：

- 一行一个 JSON object（与 v1 的 NDJSON 保持一致）
- 列名按 query 的 projection 返回

## 6. Risks

- 如果没有 CLI 跑通链路，后续绑定（Node/Python）会各自实现一遍 query glue，成本翻倍
- 如果测试覆盖太大，M3 会被拖死；必须把范围锁死在“最小子集 + 关键语义”

