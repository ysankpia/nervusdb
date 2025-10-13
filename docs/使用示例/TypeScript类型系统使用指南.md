# 类型系统使用指南

## 目标

- 使用 `TypedNervusDB` 提供的泛型 API 和类型推断
- 演示属性索引、查询过滤在类型系统下的写法

## 定义类型

```ts
interface PersonNode {
  labels: string[];
  dept?: string;
  hireDate?: string;
}

interface RelationEdge {
  since?: string;
  strength?: number;
  tags?: string[];
}
```

## 打开数据库

```ts
const db = await TypedNervusDB.open<PersonNode, RelationEdge>('social.nervusdb', {
  enableLock: true,
});
```

## 写入

```ts
await db.addFact(
  { subject: 'user:alice', predicate: 'FRIEND_OF', object: 'user:bob' },
  {
    subjectProperties: { labels: ['Person'], dept: 'R&D' },
    objectProperties: { labels: ['Person'], dept: 'Ops' },
    edgeProperties: { since: '2023-01-01', strength: 0.9, tags: ['core'] },
  },
);
```

> IDE 会对属性类型进行提示与校验。

## 查询

```ts
const strong = await db
  .find({ predicate: 'FRIEND_OF' })
  .where((edge) => edge.edgeProperties?.strength! > 0.75)
  .limit(10)
  .all();
```

## 属性索引查询

```ts
const rnd = await db.findByNodeProperty({ propertyName: 'dept', value: 'R&D' });
const veteran = await db.findByNodeProperty({
  propertyName: 'hireDate',
  range: { max: '2020-12-31' },
});
```

## 读取属性

```ts
const nodeId = await db.getNodeId('user:alice');
if (nodeId !== undefined) {
  const props = await db.getNodeProperties(nodeId);
  console.log(props?.dept); // 类型安全
}
```

## 常见问题

| 现象             | 原因                                | 解决                                             |
| ---------------- | ----------------------------------- | ------------------------------------------------ |
| 属性类型报错     | JSON 中包含额外字段                 | 调整接口或使用索引签名 `Record<string, unknown>` |
| 泛型过深         | 类型复杂导致推断慢                  | 拆分接口或使用 `type` 简化                       |
| 混合使用原始 API | Typed 与非 Typed 混合，缺失类型约束 | 保持统一入口或显式断言                           |

## 延伸阅读

- [教程 02 · 数据模型与基础 CRUD](../教学文档/教程-02-数据模型与基础CRUD.md)
- [docs/使用示例/03-查询与联想-示例.md](03-查询与联想-示例.md)
