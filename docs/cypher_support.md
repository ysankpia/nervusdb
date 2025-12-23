# Cypher 支持范围（子集）

NervusDB 的 Cypher 是“够用就行”的子集实现：目标是 **嵌入式读写 + 低开销绑定**，不是复刻 Neo4j。

如果你想跑百万行结果：别用一次性返回的 API。用 Statement（`prepare → step → column_*`），让数据按需进入用户态。

## 已支持（以测试为准）

### 读取

- `MATCH (n)` / `MATCH (n:Label)`
- 关系模式：`MATCH (a)-[r]->(b)`（支持链式多跳）
- `WHERE`：
  - 属性访问：`n.age` / `n.name`
  - 比较：`=, >, >=, <, <=`
  - 逻辑：`AND, OR`
  - 算术：`+`（用于表达式）
  - 参数绑定：`$param`（通过 `executeQuery(..., params)` / `prepareV2(..., params)` 传入）
- `RETURN`：
  - 变量：`RETURN n`
  - 属性：`RETURN n.name, n.age`
  - 别名：`RETURN n AS x` / `RETURN n.name AS name`

### 写入（基础）

- `CREATE`（单条 CREATE 语句）
  - 节点：`CREATE (n:Person)`
  - 节点属性：`CREATE (n:Person {name: "Alice", age: 25.0})`
  - 关系：`CREATE (a)-[:KNOWS]->(b)` / `CREATE (a)-[r:KNOWS {since: 2020}]->(b)`
- `SET`：
  - `SET n.age = 30.0`
  - `SET n.age = n.age + 5.0`
  - 多字段：`SET n.age = 28.0, n.city = "NYC"`
- `DELETE` / `DETACH DELETE`

## 输出值类型

Statement 的 `columnType()` / `column_*()` 对齐 C ABI（T10）：

- `Null`
- `Text`
- `Float`
- `Bool`
- `Node`：节点 ID（`u64` / JS `bigint`）
- `Relationship`：三元组 `{subjectId,predicateId,objectId}`（边的编码）

## 列名与顺序规则

- 有 `RETURN`：按 `RETURN` 的顺序输出列；如果出现重名列会报错（要求显式 alias）。
- 无 `RETURN`：从结果集 key 集合推导，按字典序稳定排序（用于保持确定性）。

## 不支持 / 已知限制

- 这不是完整 Cypher：未承诺覆盖 Neo4j 的全部语法与优化器行为。
- 大结果集建议走 Statement；一次性 `executeQuery()` 会导致 JS 堆分配/GC 压力（尤其是百万行级别）。

