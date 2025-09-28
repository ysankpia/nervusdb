<docs>
# API 参考

<cite>
**本文档中引用的文件**   
- [synapseDb.ts](file://src/synapseDb.ts)
- [index.ts](file://src/index.ts)
- [queryBuilder.ts](file://src/query/queryBuilder.ts)
- [typedSynapseDb.ts](file://src/typedSynapseDb.ts)
- [types/enhanced.ts](file://src/types/enhanced.ts)
- [query/cypher.ts](file://src/query/cypher.ts)
- [query/graphql/index.ts](file://src/query/graphql/index.ts)
- [query/gremlin/index.ts](file://src/query/gremlin/index.ts)
- [types/openOptions.ts](file://src/types/openOptions.ts) - *新增事务与快照相关选项*
- [tests/helpers/tempfs.ts](file://tests/helpers/tempfs.ts) - *新增测试辅助工具*
</cite>

## 更新摘要
**变更内容**   
- 新增“事务与快照API”章节，详细说明 `beginBatch`、`commitBatch`、`abortBatch` 和 `withSnapshot` 方法
- 在“核心数据库类 SynapseDB”中补充了事务批次和幂等性配置说明
- 更新“接口定义摘要”以包含新的事务和快照相关接口
- 添加 `SynapseDBOpenOptions` 配置项的完整文档
- **新增测试辅助模块说明**：添加对 `tempfs.ts` 测试助手的描述，用于隔离测试环境

## 目录
1. [核心数据库类 SynapseDB](#核心数据库类-synapsedb)
2. [查询构建器 QueryBuilder](#查询构建器-querybuilder)
3. [类型安全数据库 TypedSynapseDB](#类型安全数据库-typedsynapsedb)
4. [高级查询语言支持](#高级查询语言支持)
5. [事务与快照API](#事务与快照api)
6. [接口定义摘要](#接口定义摘要)

## 核心数据库类 SynapseDB

`SynapseDB` 是嵌入式三元组知识库的核心类，提供打开、关闭数据库以及增删查改事实记录的功能。

### 打开与关闭数据库
`open` 方法用于打开或创建一个数据库实例。如果指定路径的数据库文件不存在，则会自动创建。

```typescript
const db = await SynapseDB.open('./my-database.synapsedb');
```

`close` 方法用于关闭数据库连接并释放资源。

```typescript
await db.close();
```

**节来源**
- [synapseDb.ts](file://src/synapseDb.ts#L57-L915)

### 添加和删除事实
`addFact` 方法用于向数据库中添加一个新的事实（SPO三元组），并可选择性地为节点和边设置属性。

```typescript
db.addFact(
  { subject: 'Alice', predicate: 'knows', object: 'Bob' },
  {
    subjectProperties: { age: 30 },
    edgeProperties: { since: new Date() }
  }
);
```

`deleteFact` 方法根据给定的事实描述删除对应的记录。

```typescript
db.deleteFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
```

**节来源**
- [synapseDb.ts](file://src/synapseDb.ts#L57-L915)

### 基本查询操作
`find` 方法根据条件查找匹配的事实记录，并返回一个 `QueryBuilder` 实例以支持链式调用。

```typescript
const results = db.find({ predicate: 'knows' }).all();
```

`findByNodeProperty` 和 `findByEdgeProperty` 分别基于节点或边的属性进行查询。

```typescript
// 查找年龄为25的用户
const users = db.findByNodeProperty({ propertyName: 'age', value: 25 }).all();

// 查找权重为0.8的关系
const strongRelations = db.findByEdgeProperty({ propertyName: 'weight', value: 0.8 }).all();
```

`findByLabel` 方法根据节点标签进行查询。

```typescript
const persons = db.findByLabel('Person').all();
```

**节来源**
- [synapseDb.ts](file://src/synapseDb.ts#L57-L915)

### 数据库打开选项
`SynapseDBOpenOptions` 接口提供了丰富的数据库配置选项：

```typescript
interface SynapseDBOpenOptions {
  /**
   * 索引目录路径
   * @default `${dbPath}.pages`
   */
  indexDirectory?: string;

  /**
   * 页面大小（三元组数量）
   * @default 1000
   */
  pageSize?: number;

  /**
   * 是否重建索引
   * @default false
   */
  rebuildIndexes?: boolean;

  /**
   * 压缩选项
   * @default { codec: 'none' }
   */
  compression?: {
    codec: 'none' | 'brotli';
    level?: number;
  };

  /**
   * 启用进程级独占写锁
   * @default false
   */
  enableLock?: boolean;

  /**
   * 注册为读者
   * @default true（自 v2 起）
   */
  registerReader?: boolean;

  /**
   * 暂存模式
   * @default 'default'
   */
  stagingMode?: 'default' | 'lsm-lite';

  /**
   * 启用跨周期 txId 幂等去重
   * @default false
   */
  enablePersistentTxDedupe?: boolean;

  /**
   * 记忆的最大事务 ID 数量
   * @default 1000
   */
  maxRememberTxIds?: number;
}
```

这些选项允许精细控制数据库的行为、性能和并发特性。

**节来源**
- [types/openOptions.ts](file://src/types/openOptions.ts#L1-L153)

## 查询构建器 QueryBuilder

`QueryBuilder` 提供了链式的查询构造能力，允许通过一系列方法组合来精确控制查询结果。

### 联想查询
`follow` 和 `followReverse` 方法分别实现正向和反向的联想查询。

```typescript
// 查找 Alice 的朋友的朋友
const friendsOfFriends = db.find({ subject: 'Alice' })
  .follow('knows')
  .follow('knows')
  .all();
```

### 条件过滤
`where` 方法接受一个谓词函数，对当前结果集中的每条记录进行过滤。

```typescript
// 过滤出年龄大于25的用户
const adults = db.findByNodeProperty({ propertyName: 'age' })
  .where(record => (record.subjectProperties?.age as number) > 25)
  .all();
```

### 结果限制
`limit` 和 `skip` 方法用于分页或限制返回的结果数量。

```typescript
// 获取前10条记录
const top10 = db.find({}).limit(10).all();

// 跳过前5条记录
const page2 = db.find({}).skip(5).limit(5).all();
```

### 属性与标签过滤
`whereProperty` 方法支持基于节点或边属性的等值或范围查询。

```typescript
// 查找年龄在25到35之间的用户
const rangeQuery = db.find({})
  .whereProperty('age', '>=', 25, 'node')
  .whereProperty('age', '<=', 35, 'node');
```

`whereLabel` 方法根据节点标签进一步筛选结果。

```typescript
// 筛选出同时具有 Person 和 Employee 标签的主体
const employees = db.find({})
  .whereLabel(['Person', 'Employee'], { mode: 'AND', on: 'subject' });
```

**节来源**
- [queryBuilder.ts](file://src/query/queryBuilder.ts#L38-L812)

## 类型安全数据库 TypedSynapseDB

`TypedSynapseDB` 接口提供了泛型化的类型安全访问方式，确保在编译期就能捕获属性类型错误。

### 泛型绑定示例
通过泛型参数可以定义节点和边的属性结构。

```typescript
interface PersonNode {
  name: string;
  age?: number;
  email?: string;
}

interface RelationshipEdge {
  since: Date;
  strength: number;
}

const typedDb = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./social.db');
```

### 类型安全查询的优势
使用 `TypedSynapseDB` 后，所有涉及属性的操作都具备完整的类型检查。

```typescript
// 编译时即可发现拼写错误或类型不匹配
const result = typedDb.find({ subject: 'Alice' })
  .where(record => record.subjectProperties.age > 30) // ✅ 类型正确
  .all();
```

这避免了运行时因属性名错误或类型不符导致的问题，提升了开发效率和代码可靠性。

**节来源**
- [typedSynapseDb.ts](file://src/typedSynapseDb.ts#L0-L291)
- [types/enhanced.ts](file://src/types/enhanced.ts#L141-L215)

## 高级查询语言支持

### Cypher 查询
`cypherQuery` 方法执行标准的 Cypher 查询语句，支持参数化和只读模式。

```typescript
const result = await db.cypherQuery(
  'MATCH (a:Person)-[r:KNOWS]->(b:Person) WHERE a.age > $minAge RETURN a, b',
  { minAge: 18 }
);
```

`validateCypher` 方法可用于验证 Cypher 语句的语法正确性。

```typescript
const validation = db.validateCypher('MATCH (n) RETURN n');
if (!validation.valid) {
  console.error(validation.errors);
}
```

### GraphQL 查询
`graphql` 函数为数据库提供自动生成的 GraphQL Schema 和查询能力。

```typescript
import { graphql } from './query/graphql';

const gqlService = graphql(db.store);
const schema = await gqlService.getSchema();
const result = await gqlService.executeQuery(`{
  persons(name: "Alice") {
    name
    friends {
      name
    }
  }
}`);
```

### Gremlin 查询
`gremlin` 函数提供兼容 Apache TinkerPop 的 Gremlin 遍历 API。

```typescript
import { gremlin } from './query/gremlin';

const g = gremlin(db.store);
const results = await g.V()
  .has('name', 'Alice')
  .out('knows')
  .values('name')
  .toList();
```

**节来源**
- [query/cypher.ts](file://src/query/cypher.ts#L0-L286)
- [query/graphql/index.ts](file://src/query/graphql/index.ts#L0-L331)
- [query/gremlin/index.ts](file://src/query/gremlin/index.ts#L0-L283)

## 事务与快照API

### 事务批次控制
`SynapseDB` 支持显式的事务批次控制，允许将多个写入操作组合成一个原子批次。

#### 开始批次
`beginBatch` 方法开始一个新的事务批次，可选地指定 `txId` 和 `sessionId` 用于幂等性控制。

```typescript
db.beginBatch({ 
  txId: 'transaction-001', 
  sessionId: 'writer-instance-A' 
});
```

#### 提交批次
`commitBatch` 方法提交当前批次的所有更改。可通过 `durable` 选项控制持久性保证。

```typescript
// 提交并确保数据持久化到磁盘
db.commitBatch({ durable: true });
```

#### 中止批次
`abortBatch` 方法回滚当前批次的所有更改。

```typescript
// 如果发生错误，回滚所有更改
try {
  db.beginBatch();
  db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
  db.setNodeProperties(nodeId, { status: 'active' });
  db.commitBatch();
} catch (error) {
  db.abortBatch(); // 回滚所有更改
  throw error;
}
```

**节来源**
- [synapseDb.ts](file://src/synapseDb.ts#L460-L470)

### 读快照一致性
`