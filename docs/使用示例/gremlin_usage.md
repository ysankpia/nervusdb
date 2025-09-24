# SynapseDB Gremlin 使用指南

SynapseDB 现在支持 Apache TinkerPop 兼容的 Gremlin 图遍历语言，让您可以使用熟悉的图查询语法来探索知识图谱。

## 快速开始

### 基础连接和设置

```typescript
import { SynapseDB } from '@/synapseDb';
import { gremlin, P } from '@/query/gremlin';

// 打开数据库
const db = await SynapseDB.open('knowledge-graph.synapsedb');

// 创建 Gremlin 遍历源
const g = gremlin(db.store);

// 添加一些示例数据
db.addFact({ subject: 'person:alice', predicate: 'HAS_NAME', object: '爱丽丝' });
db.addFact({ subject: 'person:alice', predicate: 'HAS_AGE', object: '28' });
db.addFact({ subject: 'person:alice', predicate: 'HAS_PROFESSION', object: '工程师' });

db.addFact({ subject: 'person:bob', predicate: 'HAS_NAME', object: '鲍勃' });
db.addFact({ subject: 'person:bob', predicate: 'HAS_AGE', object: '32' });
db.addFact({ subject: 'person:bob', predicate: 'HAS_PROFESSION', object: '设计师' });

db.addFact({ subject: 'person:alice', predicate: 'KNOWS', object: 'person:bob' });
db.addFact({ subject: 'person:bob', predicate: 'KNOWS', object: 'person:alice' });

await db.flush();
```

## 基础查询操作

### 获取所有顶点和边

```typescript
// 获取所有顶点
const allVertices = await g.V().toList();
console.log(`找到 ${allVertices.length} 个顶点`);

// 获取所有边
const allEdges = await g.E().toList();
console.log(`找到 ${allEdges.length} 条边`);
```

### 属性过滤

```typescript
// 查找具有特定属性的顶点
const peopleWithNames = await g.V().has('HAS_NAME').toList();

// 查找具有特定属性值的顶点
const alice = await g.V().has('HAS_NAME', '爱丽丝').next();

console.log('爱丽丝的信息:', alice.properties);

// 使用谓词进行条件过滤
const seniors = await g.V().has('HAS_AGE', P.gte('30')).values('HAS_NAME').toList();

console.log(
  '30岁及以上的人:',
  seniors.map((s) => s.properties.value),
);
```

### 图遍历

```typescript
// 查找朋友
const aliceFriends = await g.V().has('HAS_NAME', '爱丽丝').out('KNOWS').values('HAS_NAME').toList();

console.log(
  '爱丽丝的朋友:',
  aliceFriends.map((f) => f.properties.value),
);

// 双向关系查询
const mutualConnections = await g
  .V()
  .has('HAS_NAME', '爱丽丝')
  .both('KNOWS')
  .values('HAS_NAME')
  .toList();

// 多跳查询（朋友的朋友）
const friendsOfFriends = await g
  .V()
  .has('HAS_NAME', '爱丽丝')
  .out('KNOWS')
  .out('KNOWS')
  .has('HAS_NAME', P.neq('爱丽丝')) // 排除自己
  .dedup()
  .values('HAS_NAME')
  .toList();
```

### 边查询

```typescript
// 获取出边
const outgoingEdges = await g.V().has('HAS_NAME', '爱丽丝').outE().toList();

// 获取特定类型的边
const knowsEdges = await g.V().has('HAS_NAME', '爱丽丝').outE('KNOWS').toList();

// 从边到顶点
const targets = await g
  .V()
  .has('HAS_NAME', '爱丽丝')
  .outE('KNOWS')
  .inV()
  .values('HAS_NAME')
  .toList();
```

## 高级查询模式

### 复合条件查询

```typescript
// 查找年龄在特定范围内的工程师
const engineersInRange = await g
  .V()
  .has('HAS_PROFESSION', '工程师')
  .has('HAS_AGE', P.between('25', '35'))
  .values('HAS_NAME')
  .toList();

// 多条件组合
const criteria = await g
  .V()
  .has('HAS_PROFESSION', P.within(['工程师', '设计师']))
  .has('HAS_AGE', P.gte('25'))
  .valueMap('HAS_NAME', 'HAS_AGE', 'HAS_PROFESSION')
  .toList();
```

### 路径查询

```typescript
// 查找特定路径
const engineerFriends = await g
  .V()
  .has('HAS_PROFESSION', '工程师')
  .out('KNOWS')
  .has('HAS_PROFESSION', '设计师')
  .values('HAS_NAME')
  .toList();

// 复杂路径模式
const complexPath = await g
  .V()
  .has('HAS_NAME', '爱丽丝')
  .out('WORKS_AT') // 爱丽丝的公司
  .in('WORKS_AT') // 同事
  .has('HAS_NAME', P.neq('爱丽丝')) // 排除自己
  .out('KNOWS') // 同事的朋友
  .dedup()
  .values('HAS_NAME')
  .toList();
```

### 聚合和统计

```typescript
// 计数查询
const totalPeople = await g.V().has('HAS_PROFESSION').count().next();

console.log(`总共有 ${totalPeople.properties.value} 个人`);

// 去重统计
const uniqueProfessions = await g.V().values('HAS_PROFESSION').dedup().count().next();

console.log(`有 ${uniqueProfessions.properties.value} 种不同的职业`);

// 分组统计（手动实现）
const allProfessions = await g.V().values('HAS_PROFESSION').toList();

const professionCount = {};
allProfessions.forEach((p) => {
  const profession = p.properties.value;
  professionCount[profession] = (professionCount[profession] || 0) + 1;
});

console.log('职业分布:', professionCount);
```

## 数据建模最佳实践

### 实体标识

```typescript
// 为不同类型的实体添加类型标记
db.addFact({ subject: 'person:alice', predicate: 'TYPE', object: 'Person' });
db.addFact({ subject: 'company:techcorp', predicate: 'TYPE', object: 'Company' });
db.addFact({ subject: 'project:webapp', predicate: 'TYPE', object: 'Project' });

// 使用类型进行过滤
const people = await g.V().has('TYPE', 'Person').values('HAS_NAME').toList();

const companies = await g.V().has('TYPE', 'Company').values('HAS_NAME').toList();
```

### 关系建模

```typescript
// 对称关系（朋友关系）
db.addFact({ subject: 'person:alice', predicate: 'KNOWS', object: 'person:bob' });
db.addFact({ subject: 'person:bob', predicate: 'KNOWS', object: 'person:alice' });

// 非对称关系（工作关系）
db.addFact({ subject: 'person:alice', predicate: 'WORKS_AT', object: 'company:techcorp' });
db.addFact({ subject: 'person:alice', predicate: 'WORKS_ON', object: 'project:webapp' });

// 层次关系
db.addFact({ subject: 'person:bob', predicate: 'MANAGES', object: 'person:charlie' });
db.addFact({ subject: 'person:charlie', predicate: 'REPORTS_TO', object: 'person:bob' });
```

## 性能优化建议

### 查询优化

```typescript
// 1. 尽早使用过滤条件
// 好：先过滤再遍历
const optimized = await g
  .V()
  .has('TYPE', 'Person')
  .has('HAS_PROFESSION', '工程师')
  .out('KNOWS')
  .limit(10)
  .toList();

// 不好：先遍历再过滤
const unoptimized = await g
  .V()
  .out('KNOWS')
  .has('TYPE', 'Person')
  .has('HAS_PROFESSION', '工程师')
  .limit(10)
  .toList();

// 2. 使用 limit() 控制结果数量
const limitedResults = await g.V().has('TYPE', 'Person').limit(100).toList();

// 3. 使用 dedup() 去除重复
const uniqueResults = await g.V().out('KNOWS').out('KNOWS').dedup().limit(50).toList();
```

### 批量操作

```typescript
// 批量添加数据
const people = [
  { id: 'person:1', name: '张三', age: 25 },
  { id: 'person:2', name: '李四', age: 30 },
  { id: 'person:3', name: '王五', age: 35 },
];

for (const person of people) {
  db.addFact({ subject: person.id, predicate: 'HAS_NAME', object: person.name });
  db.addFact({ subject: person.id, predicate: 'HAS_AGE', object: person.age.toString() });
  db.addFact({ subject: person.id, predicate: 'TYPE', object: 'Person' });
}

// 一次性刷新到磁盘
await db.flush();
```

## 错误处理

```typescript
// 处理空结果
try {
  const result = await g.V().has('HAS_NAME', '不存在的人').next();
  console.log(result);
} catch (error) {
  if (error.message.includes('No more elements')) {
    console.log('未找到匹配的元素');
  } else {
    console.error('查询错误:', error);
  }
}

// 安全的获取操作
const safeResult = await g.V().has('HAS_NAME', '可能不存在的人').tryNext();

if (safeResult) {
  console.log('找到:', safeResult.properties.HAS_NAME);
} else {
  console.log('未找到匹配项');
}

// 检查是否有结果
const hasResults = await g.V().has('HAS_PROFESSION', '科学家').hasNext();

if (hasResults) {
  const scientists = await g.V().has('HAS_PROFESSION', '科学家').toList();
  console.log(`找到 ${scientists.length} 位科学家`);
}
```

## 与传统查询的对比

### Gremlin vs SynapseDB QueryBuilder

```typescript
// SynapseDB 传统方式
const traditionalResults = db
  .find({ predicate: 'HAS_NAME' })
  .follow('KNOWS')
  .where({ predicate: 'HAS_PROFESSION', object: '工程师' })
  .all();

// Gremlin 方式（更直观）
const gremlinResults = await g
  .V()
  .has('HAS_NAME')
  .out('KNOWS')
  .has('HAS_PROFESSION', '工程师')
  .toList();

// 复杂查询对比
// 传统方式：需要多个查询步骤
const step1 = db.find({ subject: 'person:alice' });
const step2 = step1.follow('KNOWS').all();
const friends = step2.map(/* 提取朋友信息 */);

// Gremlin 方式：一个链式查询
const gremlinFriends = await g
  .V()
  .has('HAS_NAME', '爱丽丝')
  .out('KNOWS')
  .values('HAS_NAME')
  .toList();
```

## 总结

SynapseDB 的 Gremlin 支持让图查询变得更加直观和强大：

- **熟悉的语法**：使用标准的 Gremlin 遍历语法
- **链式操作**：支持复杂的多步遍历
- **丰富的过滤**：支持各种属性和值过滤
- **性能优化**：与 SynapseDB 的索引系统深度集成
- **类型安全**：完整的 TypeScript 类型支持

通过 Gremlin，您可以用更自然的方式探索和查询知识图谱，无论是简单的邻居查询还是复杂的多跳路径分析。
