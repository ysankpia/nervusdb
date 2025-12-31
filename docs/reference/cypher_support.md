# Cypher 支持范围（子集）

NervusDB 的 Cypher 是"够用就行"的子集实现：目标是 **嵌入式读写 + 低开销绑定**，不是复刻 Neo4j。

> 注意：本仓库进入收尾模式，文档以“不骗人”为第一原则。请以本文件为准，不要只看 README 的旧片段。

## v2 M3 子集（当前开发版本）

v2 M3 是新一代查询引擎，支持通过 `nervusdb-v2-query` crate 或 CLI `v2 write/query` 子命令使用。

### 读取

- `RETURN 1`（常量返回）
- `EXPLAIN <query>`（仅返回编译后的 Plan；不执行 query）
- 单节点扫描：`MATCH (n) RETURN n`
- 单跳模式：`MATCH (a)-[:<u32>]->(b) RETURN a, b`
- 单跳可变长度：`MATCH (a)-[:<u32>*1..5]->(b) RETURN a, b`
- `WHERE` 属性过滤（节点/边）：`MATCH (a)-[r:1]->(b) WHERE a.name = 'Alice' AND r.weight > 1.0 RETURN a, b`
- `ORDER BY <var> [ASC|DESC]`（限制：M3 仅支持对变量排序）
- `SKIP n`
- `RETURN DISTINCT`
- `LIMIT n`（非负整数）

### 写入

- `CREATE`：
  - 单节点：`CREATE (n)` / `CREATE (n {name: 'Alice', age: 30})`
  - 单跳关系：`CREATE (a)-[:1]->(b)` / `CREATE (a {name: 'A'})-[:1 {weight: 2.5}]->(b {name: 'B'})`
- `MERGE`（幂等写入）：
  - 单节点：`MERGE (n {name: 'Alice'})`
  - 单跳关系：`MERGE (a {name: 'A'})-[:1]->(b {name: 'B'})`
- `DELETE` / `DETACH DELETE`：
  - `MATCH (n) DELETE n`（删除节点）
  - `MATCH (a)-[:1]->(b) DELETE a`（删除节点）
  - `MATCH (a)-[r:1]->(b) DELETE r`（删除边；需要给关系绑定变量）
  - `MATCH (a)-[:1]->(b) DETACH DELETE a`（先删除边，再删除节点）
  - 单次语句删除目标数量上限：`100_000`（超过会 fail-fast；请分批删除）
- `SET`（属性更新）：
  - 节点属性：`MATCH (n:Person) WHERE n.name = 'Alice' SET n.name = 'Bob'`
  - 边属性：`MATCH (a)-[r:1]->(b) SET r.since = 2024`

### 已知限制

- 仅支持单节点或单跳模式（pattern elements 为 1 或 3）；可变长度仍然必须是这一个关系上的 `*min..max`
- 关系类型目前按“字符串名字”处理（推荐使用数字：`:1`, `:2` 等）
- 标签（`:Label`）已支持于 `MATCH`/`CREATE`/`MERGE` 的最小子集；`DELETE` 仍不支持标签
- 不支持在 `MATCH` pattern 内写属性：`MATCH (a {name:'Alice'})-[:1]->(b)` 会 fail-fast（请用 WHERE）
- `MERGE` 节点必须提供非空 property map（否则没有稳定 identity）
- `DELETE` 不支持级联删除
- `WITH` / `UNWIND` / `UNION` / `CALL`：明确不支持（超出即 fail-fast）
- `OPTIONAL MATCH`：明确不支持（超出即 fail-fast）

## 可变长度的现实边界（别踩雷）

- `*` 的默认语义是 `*1..`（min 默认为 1）
- **max 省略时不会无限遍历**：执行器会用默认上限（当前为 5 hops）做硬截断
- `*0` / `*0..`：明确不支持（0 长度路径会把边界条件搞成屎）
- `*max<min`：直接报错

## 输出与类型（v2 CLI）

`nervusdb-cli v2 query` 输出 NDJSON，每行一条 JSON 记录；列名来自 `RETURN` 的变量名。

目前会出现的值类型：

- `null`
- `bool`
- `number`（整数/浮点）
- `string`
- `{"internal_node_id": <u32>, "external_id": <u64>?}`（节点 ID；外部 ID 存在时会一并输出）
- `{"src": <u32>, "rel": <u32>, "dst": <u32>}`（边 key）

## fail-fast 规则

- 这不是完整 Cypher：未承诺覆盖 Neo4j 的全部语法与优化器行为。
- **白名单之外的语法会 fail-fast**：返回 `not implemented: <feature>`。
