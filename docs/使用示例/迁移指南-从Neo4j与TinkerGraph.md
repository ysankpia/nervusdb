# 迁移指南 · 从 Neo4j / TinkerGraph

## 目标

- 将现有 Neo4j / TinkerGraph 图数据迁移到 NervusDB
- 映射标签、属性、关系到三元组与特性

## 概念映射

| Neo4j/TinkerGraph       | NervusDB                                 |
| ----------------------- | ---------------------------------------- |
| Node (label)            | `subject`/`object` + `labels` 属性       |
| Relationship (type)     | `predicate`                              |
| Relationship properties | `edgeProperties`                         |
| Node properties         | `subjectProperties` / `objectProperties` |
| ID                      | 推荐使用 `node:<id>` 字符串              |

## 数据导出

### Neo4j（cypher-shell）

```cypher
MATCH (n)-[r]->(m)
RETURN id(n) AS sid, labels(n) AS slabels, properties(n) AS sprops,
       type(r) AS type, properties(r) AS eprops,
       id(m) AS oid, labels(m) AS olabels, properties(m) AS oprops
```

将结果导出为 CSV/JSON。

### TinkerGraph（Gremlin Console）

```groovy
g.E().project('out','label','in','props','outProps','inProps')
  .by(outV())
  .by(label())
  .by(inV())
  .by(valueMap())
  .by(outV().valueMap())
  .by(inV().valueMap())
```

## 导入脚本示例

```ts
import { NervusDB } from 'nervusdb';
import edges from './neo4j-export.json' assert { type: 'json' };

const db = await NervusDB.open('neo4j-migrated.nervusdb', {
  enableLock: true,
  enablePersistentTxDedupe: true,
});

db.beginBatch({ txId: 'neo4j-import', sessionId: 'migration' });

for (const edge of edges) {
  await db.addFact(
    { subject: `node:${edge.sid}`, predicate: edge.type, object: `node:${edge.oid}` },
    {
      subjectProperties: { labels: edge.slabels, ...edge.sprops },
      objectProperties: { labels: edge.olabels, ...edge.oprops },
      edgeProperties: edge.eprops,
    },
  );
}

db.commitBatch();
await db.flush();
await db.close();
```

## 验证

- `nervusdb stats` 查看迁移后的三元组数量
- `nervusdb dump` 检查属性是否正确
- 执行关键查询与 Neo4j/TinkerGraph 结果对比

## 常见问题

| 现象     | 原因                    | 解决                              |
| -------- | ----------------------- | --------------------------------- |
| 属性丢失 | JSON 中包含嵌套复杂对象 | 展平或序列化为字符串              |
| ID 冲突  | 导入时重复字符串        | 加前缀（如 `neo4j:`）或使用 GUID  |
| 性能慢   | 数据量大                | 使用批次、分块导入、启用 LSM-Lite |

## 延伸阅读

- [docs/教学文档/教程-02-数据模型与基础CRUD.md](../教学文档/教程-02-数据模型与基础CRUD.md)
- [docs/使用示例/03-查询与联想-示例.md](03-查询与联想-示例.md)
