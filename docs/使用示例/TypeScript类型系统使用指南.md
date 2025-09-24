# TypeScript 类型系统使用指南

SynapseDB v1.1 引入了完整的 TypeScript 类型系统，提供编译时类型安全和智能代码补全，同时保持与原始 API 的运行时兼容性。

## 快速开始

```typescript
import { TypedSynapseDB, PersonNode, RelationshipEdge } from 'synapsedb';

// 打开类型化数据库
const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./social.synapsedb');

// 添加类型化数据
const friendship = db.addFact(
  { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
  {
    subjectProperties: { name: 'Alice', age: 30, labels: ['Person'] },
    objectProperties: { name: 'Bob', age: 25, labels: ['Person'] },
    edgeProperties: { since: new Date(), strength: 0.8, type: 'friend' },
  },
);

// TypeScript 能提供完整的类型提示
console.log(friendship.subjectProperties?.name); // Alice
console.log(friendship.edgeProperties?.type); // friend
```

## 预定义类型

### 社交网络类型

```typescript
import { PersonNode, RelationshipEdge } from 'synapsedb';

interface PersonNode {
  name: string;
  age?: number;
  email?: string;
  labels?: ('Person' | 'User')[];
}

interface RelationshipEdge {
  since?: Date | number;
  strength?: number;
  type?: 'friend' | 'colleague' | 'family';
}

// 使用预定义类型
const socialDb = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./social.synapsedb');
```

### 知识图谱类型

```typescript
import { EntityNode, KnowledgeEdge } from 'synapsedb';

const knowledgeDb = await TypedSynapseDB.open<EntityNode, KnowledgeEdge>('./knowledge.synapsedb');

knowledgeDb.addFact(
  { subject: 'Einstein', predicate: 'DISCOVERED', object: 'Relativity' },
  {
    subjectProperties: {
      type: 'Person',
      title: 'Albert Einstein',
      confidence: 0.99,
      labels: ['Scientist', 'Physicist'],
    },
    objectProperties: {
      type: 'Theory',
      title: 'Theory of Relativity',
      confidence: 0.95,
      labels: ['Physics', 'Theory'],
    },
    edgeProperties: {
      confidence: 0.98,
      source: 'scientific_literature',
      timestamp: Date.now(),
      weight: 1.0,
    },
  },
);
```

### 代码依赖类型

```typescript
import { CodeNode, DependencyEdge } from 'synapsedb';

const codeDb = await TypedSynapseDB.open<CodeNode, DependencyEdge>('./dependencies.synapsedb');

codeDb.addFact(
  { subject: 'src/utils.ts', predicate: 'IMPORTS', object: 'lodash' },
  {
    subjectProperties: {
      path: 'src/utils.ts',
      type: 'file',
      language: 'typescript',
      size: 1024,
      labels: ['utility', 'helper'],
    },
    objectProperties: {
      path: 'node_modules/lodash',
      type: 'module',
      language: 'javascript',
      labels: ['library', 'external'],
    },
    edgeProperties: {
      type: 'imports',
      line: 1,
      column: 0,
    },
  },
);
```

## 自定义类型

```typescript
// 定义自定义节点和边类型
interface MyNode {
  title: string;
  score: number;
  tags?: string[];
}

interface MyEdge {
  weight: number;
  color: 'red' | 'blue' | 'green';
  metadata?: Record<string, any>;
}

// 使用自定义类型
const customDb = await TypedSynapseDB.open<MyNode, MyEdge>('./custom.synapsedb');

const result = customDb.addFact(
  { subject: 'NodeA', predicate: 'CONNECTS', object: 'NodeB' },
  {
    subjectProperties: { title: 'First Node', score: 100, tags: ['important'] },
    objectProperties: { title: 'Second Node', score: 85 },
    edgeProperties: { weight: 0.7, color: 'blue', metadata: { created: Date.now() } },
  },
);

// TypeScript 提供完整的类型安全
result.subjectProperties?.title; // string
result.edgeProperties?.weight; // number
result.edgeProperties?.color; // 'red' | 'blue' | 'green'
```

## 类型安全查询

### 基本查询

```typescript
const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./social.synapsedb');

// 基于条件查询
const friends = db
  .find({ predicate: 'FRIEND_OF' })
  .where((record) => record.edgeProperties?.strength! > 0.5)
  .limit(10)
  .all();

// friends 的类型为 TypedFactRecord<PersonNode, RelationshipEdge>[]
friends.forEach((friend) => {
  console.log(friend.subjectProperties?.name); // 类型安全
  console.log(friend.edgeProperties?.type); // 类型安全
});
```

### 属性查询

```typescript
// 精确值查询
const adults = db.findByNodeProperty({ propertyName: 'age', value: 30 }).all();

// 范围查询
const youngAdults = db
  .findByNodeProperty({
    propertyName: 'age',
    range: { min: 18, max: 35, includeMin: true, includeMax: false },
  })
  .all();

// 边属性查询
const strongConnections = db
  .findByEdgeProperty({
    propertyName: 'strength',
    range: { min: 0.8, max: 1.0 },
  })
  .all();
```

### 链式查询

```typescript
// 找到朋友的朋友
const friendsOfFriends = db
  .find({ subject: 'Alice' })
  .follow('FRIEND_OF')
  .follow('FRIEND_OF')
  .where((record) => record.object !== 'Alice') // 排除自己
  .all();
```

### 标签查询

```typescript
// 单标签查询
const persons = db.findByLabel('Person').all();

// 多标签 AND 查询
const employees = db.findByLabel(['Person', 'Employee'], { mode: 'AND' }).all();

// 多标签 OR 查询
const workers = db.findByLabel(['Employee', 'Manager'], { mode: 'OR' }).all();
```

## 类型安全的辅助工具

### TypeSafeQueries

```typescript
import { TypeSafeQueries } from 'synapsedb';

// 创建类型安全的属性过滤器
const nameFilter = TypeSafeQueries.propertyFilter('name', 'Alice');
const ageRange = TypeSafeQueries.rangeFilter('age', 20, 40, {
  includeMin: true,
  includeMax: false,
});

// 使用过滤器查询
const results = db.findByNodeProperty(ageRange).all();
```

## 异步迭代器支持

```typescript
// 使用 for await 循环处理大量数据
const query = db.find({ predicate: 'FRIEND_OF' });

for await (const record of query) {
  // 逐条处理，节省内存
  console.log(`${record.subjectProperties?.name} -> ${record.objectProperties?.name}`);

  // 类型安全的访问
  if (record.edgeProperties?.strength && record.edgeProperties.strength > 0.8) {
    console.log('Strong connection found!');
  }
}
```

## 属性直接操作

```typescript
// 获取节点属性（类型安全）
const nodeProps: PersonNode | null = db.getNodeProperties(nodeId);
if (nodeProps) {
  console.log(nodeProps.name, nodeProps.age);
}

// 获取边属性（类型安全）
const edgeProps: RelationshipEdge | null = db.getEdgeProperties({
  subjectId: 1,
  predicateId: 2,
  objectId: 3,
});

// 设置属性（类型安全）
db.setNodeProperties(nodeId, {
  name: 'Updated Name',
  age: 31,
  email: 'new@example.com',
});

db.setEdgeProperties(
  { subjectId: 1, predicateId: 2, objectId: 3 },
  { strength: 0.9, type: 'family', since: new Date() },
);
```

## 原始 API 访问

需要访问底层 API 时，可以通过 `raw` 属性：

```typescript
const typedDb = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./db.synapsedb');

// 访问原始 SynapseDB 实例
const rawDb = typedDb.raw;

// 使用原始 API（无类型安全）
const rawFact = rawDb.addFact({ subject: 'A', predicate: 'B', object: 'C' });
```

## 包装现有数据库

```typescript
import { SynapseDB } from 'synapsedb';

// 打开原始数据库
const rawDb = await SynapseDB.open('./existing.synapsedb');

// 包装为类型化版本
const typedDb = TypedSynapseDB.wrap<PersonNode, RelationshipEdge>(rawDb);

// 现在可以使用类型安全的 API
const friends = typedDb.find({ predicate: 'FRIEND_OF' }).all();
```

## 最佳实践

### 1. 定义清晰的类型结构

```typescript
// 好：清晰的类型定义
interface UserNode {
  id: string;
  name: string;
  email: string;
  createdAt: Date;
  isActive: boolean;
  metadata?: {
    lastLogin?: Date;
    preferences?: Record<string, unknown>;
  };
}

interface InteractionEdge {
  type: 'like' | 'comment' | 'share' | 'follow';
  timestamp: Date;
  weight: number;
  context?: string;
}
```

### 2. 使用联合类型处理多种节点类型

```typescript
interface UserNode {
  nodeType: 'user';
  name: string;
  email: string;
}

interface PostNode {
  nodeType: 'post';
  title: string;
  content: string;
  publishedAt: Date;
}

type SocialNode = UserNode | PostNode;

const socialDb = await TypedSynapseDB.open<SocialNode, InteractionEdge>('./social.synapsedb');
```

### 3. 利用泛型进行更灵活的设计

```typescript
interface BaseNode {
  id: string;
  createdAt: Date;
  labels?: string[];
}

interface TimestampedEdge {
  timestamp: Date;
  weight?: number;
}

// 泛型数据库工厂
function createTypedDatabase<TNode extends BaseNode, TEdge extends TimestampedEdge>(path: string) {
  return TypedSynapseDB.open<TNode, TEdge>(path);
}
```

### 4. 类型守卫增强运行时安全

```typescript
function isPersonNode(node: unknown): node is PersonNode {
  return typeof node === 'object' && node !== null && typeof (node as PersonNode).name === 'string';
}

// 使用类型守卫
const nodeProps = db.getNodeProperties(nodeId);
if (isPersonNode(nodeProps)) {
  // 现在 nodeProps 确定是 PersonNode 类型
  console.log(nodeProps.name.toUpperCase());
}
```

## 性能考虑

类型化包装器在运行时几乎没有性能开销：

- **类型转换**：仅在接口层进行，无深拷贝
- **查询执行**：直接委托给原始实现
- **内存使用**：包装器本身占用极少内存
- **编译时优化**：TypeScript 编译后类型信息被擦除

## 迁移指南

从原始 API 迁移到类型化 API：

```typescript
// 之前（原始 API）
const db = await SynapseDB.open('./db.synapsedb');
const fact = db.addFact(
  { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
  { subjectProperties: { name: 'Alice', age: 30 } },
);

// 之后（类型化 API）
const typedDb = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./db.synapsedb');
const typedFact = typedDb.addFact(
  { subject: 'Alice', predicate: 'FRIEND_OF', object: 'Bob' },
  { subjectProperties: { name: 'Alice', age: 30 } },
);

// 行为完全相同，但有类型安全
```

## 常见问题

### Q: 类型化 API 与原始 API 兼容吗？

A: 完全兼容。类型化 API 是原始 API 的包装器，运行时行为完全相同。

### Q: 可以混合使用类型化和原始 API 吗？

A: 可以。通过 `.raw` 属性访问原始实例，或使用 `TypedSynapseDB.wrap()` 包装现有实例。

### Q: 类型检查在运行时生效吗？

A: TypeScript 类型仅在编译时检查。运行时类型验证需要额外实现类型守卫。

### Q: 如何处理动态类型？

A: 使用联合类型、泛型约束或 `unknown` 类型，结合运行时类型守卫。

这个类型系统为 SynapseDB 带来了现代 TypeScript 开发体验，同时保持了与现有代码的完全兼容性。
