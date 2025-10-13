# 附录 · API 参考

> 本附录汇总 NervusDB 核心 TypeScript API，适合作为开发时的速查表。所有示例均基于 ESM 模块。

## 引入方式

```ts
import { NervusDB } from 'nervusdb';
import { TypedNervusDB } from 'nervusdb';
```

## `NervusDB.open(path, options)`

| 选项                       | 类型                                          | 默认                | 说明                       |
| -------------------------- | --------------------------------------------- | ------------------- | -------------------------- |
| `indexDirectory`           | `string`                                      | `path + '.pages'`   | 分页索引目录               |
| `pageSize`                 | `number`                                      | 1024                | 索引页大小（三元组数量）   |
| `enableLock`               | `boolean`                                     | `false`             | 进程级写锁，生产建议开启   |
| `registerReader`           | `boolean`                                     | `true`              | 是否登记读者，保障治理安全 |
| `enablePersistentTxDedupe` | `boolean`                                     | `false`             | 启用事务 ID 幂等注册表     |
| `maxRememberTxIds`         | `number`                                      | 1000                | 注册表容量                 |
| `stagingMode`              | `'default' \| 'lsm-lite'`                     | `'default'`         | 启用 LSM-Lite 暂存         |
| `compression`              | `{ codec: 'none' \| 'brotli'; level?: 1-11 }` | `{ codec: 'none' }` | 索引页压缩策略             |
| `experimental`             | `{ cypher?: boolean }`                        | `{}`                | 实验性功能开关             |

## 写入与删除

```ts
await db.addFact(fact, props?);
await db.deleteFact(fact);
await db.flush();
await db.close();
```

- `fact`：`{ subject: string; predicate: string; object: string }`
- `props`：`{ subjectProperties?: object; objectProperties?: object; edgeProperties?: object }`

## 批次操作

```ts
db.beginBatch({ txId?: string; sessionId?: string });
db.commitBatch({ durable?: boolean });
db.abortBatch();
```

## 读取 API

| 方法                                                      | 返回                         | 说明           |
| --------------------------------------------------------- | ---------------------------- | -------------- |
| `find(criteria, options?)`                                | `QueryBuilder`               | 链式查询入口   |
| `streamFacts(criteria, batchSize?)`                       | `AsyncIterable<Fact[]>`      | 分批遍历事实   |
| `listFacts()`                                             | `AsyncIterable<Fact>`        | 遍历全部事实   |
| `getNodeId(value)`                                        | `Promise<number\|undefined>` | 获取节点 ID    |
| `getNodeValue(id)`                                        | `Promise<string\|undefined>` | 反向查询字符串 |
| `getNodeProperties(id)`                                   | `Promise<object\|undefined>` | 读取节点属性   |
| `getEdgeProperties({ subjectId, predicateId, objectId })` | `Promise<object\|undefined>` | 读取边属性     |

## QueryBuilder 常用方法

| 方法                             | 说明                              |
| -------------------------------- | --------------------------------- |
| `follow(predicate)`              | 顺向跳转                          |
| `followReverse(predicate)`       | 反向跳转                          |
| `where(fn)`                      | 过滤节点/边属性                   |
| `limit(n)` / `skip(n)`           | 限制条数                          |
| `distinct()`                     | 结果去重                          |
| `anchor`（在 `find` options 中） | `'subject' \| 'object' \| 'both'` |
| `stream({ batchSize })`          | Streaming 输出                    |
| `all()` / `first()`              | 获取结果                          |

## 聚合 API

```ts
const result = await db
  .aggregate()
  .match({ predicate: 'FRIEND_OF' })
  .groupBy(['subject'])
  .count('friendCount')
  .sum('totalWeight', (edge) => edge.edgeProperties?.weight as number)
  .execute();
```

- 支持 `count`、`sum`、`avg`、`min`、`max`
- 可搭配 `having`、`orderBy`、`limit`

## 属性索引

```ts
await db.findByNodeProperty({ propertyName: 'dept', value: 'R&D' });
await db.findByNodeProperty({ propertyName: 'age', range: { min: 25, max: 35 } });
await db.findByEdgeProperty({ propertyName: 'tags', contains: 'core-team' });
```

## 全文检索

```ts
import { FulltextEngine } from 'nervusdb/fulltext';
const engine = await FulltextEngine.open('demo.nervusdb');
await engine.batchIndex([{ id: 'doc:1', title: 'WAL', body: '事务日志' }]);
const hits = await engine.search({ keywords: ['事务'], limit: 10 });
```

## 空间查询

```ts
import { SpatialStore } from 'nervusdb/spatial';
const spatial = await SpatialStore.open('demo.nervusdb');
await spatial.insertGeometry('cluster:01', { type: 'Point', coordinates: [121.5, 31.2] });
const matches = await spatial.searchWithin({
  type: 'Circle',
  center: [121.5, 31.2],
  radius: 5_000,
});
```

## TypedNervusDB

```ts
interface NodeProps {
  labels: string[];
  owner?: string;
}
interface EdgeProps {
  since?: string;
  strength?: number;
}

const typed = await TypedNervusDB.open<NodeProps, EdgeProps>('demo.nervusdb');
const res = await typed
  .find({ predicate: 'FRIEND_OF' })
  .where((edge) => edge.edgeProperties?.strength! > 0.8)
  .all();
```

- 所有 `subjectProperties`、`edgeProperties` 均具备类型提示
- 支持自定义边/节点扩展方法

## 工具函数

| 函数                        | 说明                   |
| --------------------------- | ---------------------- |
| `ensureConnectionOptions`   | 校验连接配置（CLI）    |
| `buildConnectionUri`        | 生成连接 URI           |
| `sanitizeConnectionOptions` | 遮蔽口令，仅保留末四位 |

## CLI 与脚本

- CLI 参考：`docs/教学文档/附录-CLI参考.md`
- 脚本示例：`scripts/`、`benchmarks/`

## 参考

- `src/synapseDb.ts`、`src/typedSynapseDb.ts`
- 类型定义：`src/types/openOptions.ts`、`src/types/enhanced.ts`
- 特性示例：`tests/unit/**`、`tests/integration/**`
