# Cypher 支持范围（子集）

NervusDB 的 Cypher 是"够用就行"的子集实现：目标是 **嵌入式读写 + 低开销绑定**，不是复刻 Neo4j。

## v2 M3 子集（当前开发版本）

v2 M3 是新一代查询引擎，支持通过 `nervusdb-v2-query` crate 或 CLI `v2 write/query` 子命令使用。

### 读取

- `RETURN 1`（常量返回）
- 单跳模式：`MATCH (a)-[:<u32>]->(b) RETURN a, b`
- `WHERE` 属性过滤：`MATCH (a)-[:1]->(b) WHERE a.name = 'Alice' RETURN a, b`
- `LIMIT n`（非负整数）

### 写入

- `CREATE`：
  - 单节点：`CREATE (n)` / `CREATE (n {name: 'Alice', age: 30})`
  - 单跳关系：`CREATE (a)-[:1]->(b)` / `CREATE (a {name: 'A'})-[:1 {weight: 2.5}]->(b {name: 'B'})`
- `DELETE` / `DETACH DELETE`：
  - `MATCH (a)-[:1]->(b) DELETE a`（删除节点）
  - `MATCH (a)-[:1]->(b) DETACH DELETE a`（先删除边，再删除节点）

### 已知限制

- 仅支持单跳模式（3 个 pattern elements）
- 关系类型必须是数字（`:1`, `:2` 等）
- 不支持标签（`:Label`）
- 不支持变量长度路径
- `CREATE` 不支持 MERGE
- `DELETE` 不支持级联删除

---

### v1 子集（稳定版本）

v1 通过 `nervusdb-core` crate 提供，是成熟的查询引擎。

#### v1 读取操作

- `MATCH (n)` / `MATCH (n:Label)`
- 关系模式：`MATCH (a)-[r]->(b)`（支持链式多跳）
- `WHERE`：
  - 属性访问：`n.age` / `n.name`
  - 比较：`=, >, >=, <, <=`
  - 逻辑：`AND, OR`
  - 算术：`+`（用于表达式）
  - 参数绑定：`$param`
- `RETURN`：
  - 变量：`RETURN n`
  - 属性：`RETURN n.name, n.age`
  - 别名：`RETURN n AS x`
- `LIMIT n`

#### v1 写入操作

- `CREATE`（单条语句）
  - 节点：`CREATE (n:Person)`
  - 节点属性：`CREATE (n:Person {name: "Alice", age: 25.0})`
  - 关系：`CREATE (a)-[:KNOWS]->(b)` / `CREATE (a)-[r:KNOWS {since: 2020}]->(b)`
- `SET`：
  - `SET n.age = 30.0`
  - `SET n.age = n.age + 5.0`
  - 多字段：`SET n.age = 28.0, n.city = "NYC"`
- `DELETE` / `DETACH DELETE`

## 输出值类型

Statement 的 `columnType()` / `column_*()` 对齐 C ABI：

- `Null`
- `Text`
- `Float`
- `Bool`
- `Node`：节点 ID（`u64` / JS `bigint`）
- `Relationship`：三元组 `{subjectId,predicateId,objectId}`

## 列名与顺序规则

- 有 `RETURN`：按 `RETURN` 的顺序输出列；如果出现重名列会报错（要求显式 alias）。
- 无 `RETURN`：从结果集 key 集合推导，按字典序稳定排序。

## 不支持 / 已知限制

- 这不是完整 Cypher：未承诺覆盖 Neo4j 的全部语法与优化器行为。
- **白名单之外的语法会 fail-fast**：返回 `not implemented: <feature>`。
- 明确不支持：
  - `OPTIONAL MATCH`
  - `WITH`
  - `UNION`
  - `ORDER BY`
  - `SKIP`
  - `RETURN DISTINCT`
