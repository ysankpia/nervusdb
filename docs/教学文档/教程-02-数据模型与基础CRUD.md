# 教程 02 · 数据模型与基础 CRUD

## 目标

- 理解 NervusDB 的数据模型：三元组、属性、节点 ID、边 ID
- 掌握基础写入、查询、删除与 flush 流程
- 使用事务批次与类型安全包装器完成一次完整写入

## 前置要求

- 已完成 [教程 01 · 安装与环境](教程-01-安装与环境.md)
- 拥有演示库 `demo.nervusdb`

## 核心概念

| 概念    | 说明                                                             |
| ------- | ---------------------------------------------------------------- |
| Fact    | `{ subject, predicate, object }` 构成的三元组，内部映射为数字 ID |
| Node ID | `getNodeId(value)` 得到的整数标识，对应字符串值                  |
| 属性    | 节点或边附带的 JSON 文档，存储在属性区并带 `__v` 版本号          |
| 批次    | `beginBatch` ~ `commitBatch/abortBatch`，写入 WAL v2             |
| flush   | 持久化数据并触发增量索引合并，重置 WAL                           |

## 操作步骤

### 1. 写入三元组

```ts
import { NervusDB } from 'nervusdb';

const db = await NervusDB.open('demo.nervusdb');
await db.addFact({ subject: 'user:alice', predicate: 'FRIEND_OF', object: 'user:bob' });
await db.addFact({ subject: 'user:bob', predicate: 'FRIEND_OF', object: 'user:carol' });
```

### 2. 写入属性

```ts
await db.addFact(
  { subject: 'user:alice', predicate: 'WORKS_AT', object: 'team:rnd' },
  {
    subjectProperties: { labels: ['Person'], title: 'Staff Engineer' },
    objectProperties: { labels: ['Team'], manager: 'user:bob' },
    edgeProperties: { since: '2023-01-01', strength: 0.9 },
  },
);
```

### 3. 查询与取值

```ts
const facts = await db.find({ subject: 'user:alice' }).all();
for (const fact of facts) {
  console.log(fact.subject, fact.predicate, fact.object);
  console.log(fact.subjectProperties, fact.edgeProperties);
}

const nodeId = await db.getNodeId('user:alice');
const nodeProps = await db.getNodeProperties(nodeId);
```

### 4. 删除事实

```ts
await db.deleteFact({ subject: 'user:bob', predicate: 'FRIEND_OF', object: 'user:carol' });
```

> 删除操作写入 tombstone，后续 compaction/GC 会清理。

### 5. 批次写入

```ts
db.beginBatch({ txId: 'tx-2025-0001', sessionId: 'ingest-service' });
db.addFact({ subject: 'repo:core', predicate: 'DEPENDS_ON', object: 'repo:storage' });
db.addFact({ subject: 'repo:core', predicate: 'DEPENDS_ON', object: 'repo:query' });
db.commitBatch();
```

### 6. flush 与关闭

```ts
await db.flush();
await db.close();
```

## 类型安全包装器

```ts
import { TypedNervusDB } from '@/typedSynapseDb';

interface NodeProps {
  labels: string[];
  kind: 'Person' | 'Team' | 'Repo';
}
interface EdgeProps {
  since?: string;
  weight?: number;
}

const typed = await TypedNervusDB.open<NodeProps, EdgeProps>('demo.nervusdb');
const repos = await typed
  .find({ predicate: 'DEPENDS_ON' })
  .where((edge) => edge.edgeProperties?.weight! >= 0.5)
  .limit(10)
  .all();
```

## 插件系统

NervusDB 的高级查询能力由插件提供，以下插件在 `open()` 时**自动加载**：

### 默认插件

1. **PathfindingPlugin**：提供最短路径算法

   ```ts
   const path = db.shortestPath('user:alice', 'user:bob', {
     predicates: ['FRIEND_OF'],
     maxHops: 5,
   });
   ```

2. **AggregationPlugin**：提供聚合查询能力
   ```ts
   const stats = await db
     .aggregate()
     .match({ predicate: 'WORKS_AT' })
     .groupBy(['object'])
     .count('memberCount')
     .execute();
   ```

### 实验性插件

**CypherPlugin** 需显式启用：

```ts
const db = await NervusDB.open('demo.nervusdb', {
  experimental: { cypher: true },
});

const result = await db.cypher(`
  MATCH (p:Person)-[:FRIEND_OF]->(f)
  RETURN f LIMIT 10
`);
```

详见 [插件系统使用指南](../使用示例/插件系统使用指南.md)。

## 验证

- `nervusdb stats demo.nervusdb --summary` 中文件数、墓碑、热度符合预期
- `nervusdb dump demo.nervusdb SPO <primary>` 可看到新增事实与 tombstone

## 常见问题

| 情况             | 原因                              | 解决                                       |
| ---------------- | --------------------------------- | ------------------------------------------ |
| 查询结果为空     | 三元组未写入或未 flush            | 检查 `addFact` 是否成功、执行 `db.flush()` |
| 属性未更新       | 版本冲突或 JSON 不合法            | 确保属性为纯 JSON；查看 `__v` 版本控制     |
| `getNodeId` 报错 | 节点不存在                        | 先写入或处理 `undefined` 结果              |
| 批次 commit 超时 | 未调用 `commitBatch` 或被异常中断 | 适时 `abortBatch` 并重试                   |

## 延伸阅读

- [教程 03 · 查询与链式联想](教程-03-查询与链式联想.md)
- [docs/使用示例/03-查询与联想-示例.md](../使用示例/03-查询与联想-示例.md)
- [docs/使用示例/TypeScript类型系统使用指南.md](../使用示例/TypeScript类型系统使用指南.md)
