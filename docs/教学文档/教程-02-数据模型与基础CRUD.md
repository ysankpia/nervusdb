# 教程 02 · 数据模型与基础 CRUD

## 数据模型

- 事实（Fact）：`{ subject, predicate, object }`，均为字符串
- 存储时内部编码为数字 ID（三元组：`{ subjectId, predicateId, objectId }`）
- 属性：
  - 节点属性（主语/宾语）：`get/setNodeProperties(nodeId, props)`
  - 边属性（三元组键）：`get/setEdgeProperties({subjectId,predicateId,objectId}, props)`

## 基础增删改查

```ts
import { SynapseDB } from 'synapsedb';

const db = await SynapseDB.open('demo.synapsedb');

// Create：新增事实（可附带属性）
const rec = db.addFact(
  { subject: 'Alice', predicate: 'knows', object: 'Bob' },
  { edgeProperties: { weight: 1 } },
);

// Read：点查 / 列表 / 流式
const all = db.listFacts();
const knows = db.find({ predicate: 'knows' }).all();
for await (const batch of db.streamFacts({ predicate: 'knows' }, 500)) {
  // 批处理
}

// Update：属性更新（节点/边）
db.setNodeProperties(rec.subjectId, { title: 'Engineer' });
db.setEdgeProperties(
  { subjectId: rec.subjectId, predicateId: rec.predicateId, objectId: rec.objectId },
  { weight: 2 },
);

// Delete：逻辑删除（写入 tombstone，查询自动过滤）
db.deleteFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });

// 落盘持久化（字典/三元组/属性 + 分页索引合并 + WAL 重置 + 热度写入）
await db.flush();
await db.close();
```

## 事实“改写”建议

事实的 S/P/O 变动通常视为“删后加”的语义更清晰：

```ts
db.beginBatch();
db.deleteFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Carol' });
db.commitBatch();
```

## 读取属性

```ts
const propsNode = db.getNodeProperties(rec.subjectId); // Record | null
const propsEdge = db.getEdgeProperties({
  subjectId: rec.subjectId,
  predicateId: rec.predicateId,
  objectId: rec.objectId,
});
```

