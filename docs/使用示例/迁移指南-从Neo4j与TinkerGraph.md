# 迁移指南：从 Neo4j / TinkerGraph 到 SynapseDB

本文档帮助你将现有的图数据与查询从 Neo4j（Cypher）与 TinkerGraph（Gremlin）迁移到 SynapseDB。

## 数据模型映射

- 节点/边 → 三元组（subject/predicate/object）
- 标签（Label）→ 节点属性或谓语前缀（建议：`HAS_LABEL:Label` 或标签索引）
- 边属性 → `edgeProperties`（写入时附带）

## 查询映射

### 从 Cypher

Neo4j：

```
MATCH (p:Person)-[:KNOWS]->(f:Person) WHERE f.age > 25 RETURN p,f LIMIT 10
```

SynapseDB：

```ts
await db.cypherRead(
  'MATCH (p:Person)-[:KNOWS]->(f:Person) WHERE f.age > $minAge RETURN p,f LIMIT $limit',
  { minAge: 25, limit: 10 },
);
```

变长路径：

```
MATCH (a)-[:R*2..3]->(b) RETURN a,b
```

### 从 Gremlin

TinkerGraph：

```
g.V().hasLabel('Person').has('name','Alice').out('KNOWS').values('name')
```

SynapseDB：

```ts
import { gremlin } from '@/query/gremlin';
const g = gremlin(db.store);
await g.V().hasLabel('Person').has('name', 'Alice').out('KNOWS').values('name').toList();
```

## 性能与调优

- 批量写入后调用 `flush()` 合并分页索引；大规模导入建议分批次。
- 查询侧尽量使用标签/属性过滤以命中索引；长链路查询注意 LIMIT。

## 兼容性说明

- 写路径（CREATE/SET/DELETE/MERGE）逐步开放；当前优先只读查询。
- Gremlin 部分高级步骤与 SideEffect 策略按需实现；基础遍历已覆盖。

## 参考

- Cypher 语法参考：`docs/使用示例/Cypher语法参考.md`
- Gremlin 指南：`docs/使用示例/gremlin_usage.md`
- GraphQL 指南：`docs/使用示例/graphql_usage.md`
