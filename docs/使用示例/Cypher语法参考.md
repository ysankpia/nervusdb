# Cypher 语法参考（SynapseDB）

本文档总结 SynapseDB 当前支持的 Cypher 语法子集与用法示例，并给出到内部执行引擎的能力映射，便于开发与迁移。

## 支持范围（概览）

- MATCH/WHERE/RETURN/WITH 基本子句
- 节点与关系模式：`(a:Label {k:v})-[r:TYPE*min..max]->(b)`
- 变长路径：`*1..N`
- 投影/别名/聚合（示例级）
- LIMIT/ORDER BY（在编译与优化层支持）
- 参数化查询：`$param`

不支持或规划中：

- CREATE/SET/DELETE/MERGE（计划逐步开放写路径）

## 快速上手

```ts
const res = await db.cypherRead(
  'MATCH (p:Person)-[:KNOWS]->(f:Person) WHERE f.age > $minAge RETURN p,f LIMIT $limit',
  { minAge: 25, limit: 10 },
);
```

同步极简子集（兼容）：

```ts
const rows = db.cypher('MATCH (a)-[:REL*2..3]->(b) RETURN a,b');
```

## 语法要点

- 节点：`(var:Label1:Label2 {k1: v1, k2: v2})`
- 关系：`-[r:TYPE {k:v}]->`，方向 `->`/`<-`/`-`
- 变长路径：`*min..max`，如 `*1..3`
- WHERE：支持比较与属性访问，如 `a.age > 25`
- RETURN：投影变量或属性，支持别名：`RETURN a.name AS name`

## CLI 使用

```
synapsedb cypher data.synapsedb -q "MATCH (n) RETURN n LIMIT 5" --readonly
synapsedb cypher data.synapsedb --file query.cql --optimize=aggressive --params '{"minAge":25}'
```

## 能力映射

- 解析/编译/优化/执行：`src/query/pattern/*`, `src/query/cypher.ts`
- 数据库入口：`src/synapseDb.ts`（`cypherQuery/cypherRead/validateCypher`）
- 测试用例：`tests/cypher_basic.test.ts`, `tests/cypher_optimization.test.ts`, `tests/cypher_variable_path.test.ts`

## 注意事项

- 写操作默认禁用；只读模式下强制检查（`cypherRead`）。
- 大结果建议配合 LIMIT 与分页策略；或改用编程式查询流式返回。
