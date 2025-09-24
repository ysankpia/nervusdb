# GraphQL 动态 Schema 生成器使用指南

SynapseDB 提供了一个强大的 GraphQL 动态 Schema 生成器，能够从知识图谱数据中自动推断 GraphQL 类型结构，生成完整的 Schema 定义语言（SDL）和解析器。

## 快速开始

### 基础使用

```typescript
import { SynapseDB } from '@/synapseDb';
import { graphql } from '@/query/graphql';

// 创建数据库并添加数据
const db = await SynapseDB.open('knowledge.synapsedb');

// 添加实体数据
db.addFact({ subject: 'person:1', predicate: 'TYPE', object: 'Person' });
db.addFact({ subject: 'person:1', predicate: 'HAS_NAME', object: '张三' });
db.addFact({ subject: 'person:1', predicate: 'HAS_AGE', object: '30' });
db.addFact({ subject: 'person:1', predicate: 'HAS_EMAIL', object: 'zhangsan@example.com' });

// 添加关系数据
db.addFact({ subject: 'person:2', predicate: 'TYPE', object: 'Person' });
db.addFact({ subject: 'person:2', predicate: 'HAS_NAME', object: '李四' });
db.addFact({ subject: 'person:1', predicate: 'FRIEND_OF', object: 'person:2' });

await db.flush();

// 创建 GraphQL 服务
const gql = graphql(db.store);

// 获取自动生成的 Schema
const schema = await gql.getSchema();
console.log('生成的 GraphQL Schema:');
console.log(schema);
```

### 执行查询

```typescript
// 执行基础查询
const result = await gql.executeQuery(`
  query {
    persons {
      id
      name
      email
    }
  }
`);

console.log('查询结果:', result.data);

// 执行嵌套关系查询
const friendsResult = await gql.executeQuery(`
  query {
    persons {
      name
      friends {
        name
        email
      }
    }
  }
`);

console.log('朋友关系:', friendsResult.data);
```

## 核心功能

### 1. Schema 自动发现

GraphQL 生成器会自动分析 SynapseDB 中的数据结构：

- **实体类型识别**：通过 `TYPE` 谓词或结构模式推断实体类型
- **属性分析**：分析属性的数据类型（String、Int、Float、Boolean、JSON）
- **关系发现**：识别实体间的关系和连接
- **数组检测**：自动识别一对多关系

```typescript
import { discoverSchema } from '@/query/graphql';

// 独立使用 Schema 发现功能
const entityTypes = await discoverSchema(db.store, {
  maxSampleSize: 500,
  minEntityCount: 5,
});

console.log('发现的实体类型:');
entityTypes.forEach((type) => {
  console.log(`- ${type.typeName}: ${type.count} 个实例`);
  console.log(`  属性: ${type.properties.map((p) => p.fieldName).join(', ')}`);
  console.log(`  关系: ${type.relations.map((r) => r.fieldName).join(', ')}`);
});
```

### 2. 配置化 Schema 生成

```typescript
import { createGraphQLService } from '@/query/graphql';

const gql = createGraphQLService(
  db.store,
  {
    // Schema 生成配置
    minEntityCount: 10, // 最小实体数量阈值
    fieldNaming: 'camelCase', // 字段命名规范
    includeReverseRelations: true, // 包含反向关系
    maxDepth: 5, // 最大遍历深度
    excludeTypes: ['InternalType'], // 排除的类型
  },
  {
    // 解析器配置
    enablePagination: true, // 启用分页
    enableFiltering: true, // 启用过滤
    enableSorting: true, // 启用排序
    maxQueryDepth: 10, // 最大查询深度
    maxQueryComplexity: 1000, // 最大查询复杂度
  },
);
```

### 3. 分页查询

启用分页后，可以使用 Relay 规范的连接查询：

```typescript
const paginatedResult = await gql.executeQuery(
  `
  query GetPersons($first: Int, $after: String) {
    persons(first: $first, after: $after) {
      edges {
        cursor
        node {
          id
          name
          email
        }
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
      totalCount
    }
  }
`,
  {
    first: 10,
  },
);
```

### 4. 过滤和排序

```typescript
const filteredResult = await gql.executeQuery(
  `
  query GetFilteredPersons($filter: PersonFilter, $sort: [PersonSort!]) {
    persons(filter: $filter, sort: $sort) {
      name
      age
      email
    }
  }
`,
  {
    filter: {
      age_gt: 18,
      name_contains: '张',
    },
    sort: ['age_DESC', 'name_ASC'],
  },
);
```

## 生成的 Schema 结构

### 基础类型

对于每个发现的实体类型，生成器会创建：

```graphql
# 实体对象类型
type Person {
  id: ID!
  label: String
  name: String!
  age: String
  email: String
  friends: [Person!]
}

# 过滤输入类型
input PersonFilter {
  id: ID
  name: String
  name_not: String
  name_contains: String
  name_starts_with: String
  name_ends_with: String
  age: String
  age_not: String
  # ... 更多过滤条件
}

# 排序枚举
enum PersonSort {
  name_ASC
  name_DESC
  age_ASC
  age_DESC
}

# 分页连接类型
type PersonConnection {
  edges: [PersonEdge!]!
  pageInfo: PageInfo!
  totalCount: Int
}

type PersonEdge {
  node: Person!
  cursor: String!
}
```

### 根查询类型

```graphql
type Query {
  # 单个实体查询
  person(id: ID!): Person

  # 列表查询（支持分页、过滤、排序）
  persons(
    first: Int
    after: String
    last: Int
    before: String
    filter: PersonFilter
    sort: [PersonSort!]
  ): PersonConnection!

  # 其他发现的实体类型
  organization(id: ID!): Organization
  organizations(...): OrganizationConnection!
}
```

### 工具类型

```graphql
# 分页信息
type PageInfo {
  hasNextPage: Boolean!
  hasPreviousPage: Boolean!
  startCursor: String
  endCursor: String
}

# 标量类型
scalar JSON
scalar DateTime

# 枚举类型
enum SortDirection {
  ASC
  DESC
}

enum FilterOperator {
  EQ
  NEQ
  LT
  LTE
  GT
  GTE
  IN
  NOT_IN
  CONTAINS
  STARTS_WITH
  ENDS_WITH
}
```

## 高级功能

### 1. Schema 动态更新

当数据结构发生变化时，可以重新生成 Schema：

```typescript
// 添加新的实体类型
db.addFact({ subject: 'product:1', predicate: 'TYPE', object: 'Product' });
db.addFact({ subject: 'product:1', predicate: 'HAS_NAME', object: 'iPhone' });
db.addFact({ subject: 'product:1', predicate: 'HAS_PRICE', object: '999.99' });
await db.flush();

// 重新生成 Schema
await gql.regenerateSchema();

// 新的 Schema 将包含 Product 类型
const updatedSchema = await gql.getSchema();
console.log(updatedSchema); // 包含 Product 类型定义
```

### 2. 查询验证

```typescript
// 验证查询语法
const errors = await gql.validateQuery(`
  query {
    persons {
      name
      invalidField  # 这会被标记为错误
    }
  }
`);

if (errors.length > 0) {
  console.log('查询错误:', errors);
}

// 计算查询复杂度
const complexity = await gql.calculateQueryComplexity(`
  query {
    persons {
      friends {
        friends {
          name
        }
      }
    }
  }
`);

console.log('查询复杂度:', complexity);
```

### 3. 统计信息

```typescript
// 获取 Schema 统计信息
const stats = await gql.getSchemaStatistics();
console.log('Schema 统计:');
console.log(`- 类型数量: ${stats.typeCount}`);
console.log(`- 字段数量: ${stats.fieldCount}`);
console.log(`- 关系数量: ${stats.relationCount}`);
console.log(`- 分析的实体: ${stats.entitiesAnalyzed}`);
console.log(`- 生成时间: ${stats.generationTime}ms`);
console.log(`- Schema 复杂度: ${stats.schemaComplexity}`);
```

## 最佳实践

### 1. 数据建模

为了获得最佳的 GraphQL Schema，建议：

```typescript
// 使用清晰的类型声明
db.addFact({ subject: 'entity:1', predicate: 'TYPE', object: 'EntityType' });

// 使用一致的谓词命名
db.addFact({ subject: 'entity:1', predicate: 'HAS_NAME', object: 'Name' });
db.addFact({ subject: 'entity:1', predicate: 'HAS_EMAIL', object: 'email@example.com' });

// 建立清晰的关系
db.addFact({ subject: 'person:1', predicate: 'WORKS_AT', object: 'company:1' });
db.addFact({ subject: 'person:1', predicate: 'FRIEND_OF', object: 'person:2' });
```

### 2. 性能优化

```typescript
// 配置合适的采样大小
const gql = createGraphQLService(db.store, {
  maxSampleSize: 1000, // 减少大数据集的分析时间
  minEntityCount: 5, // 过滤掉少量实例的类型
});

// 使用分页避免大量数据查询
const result = await gql.executeQuery(`
  query {
    persons(first: 50) {
      edges {
        node { name }
      }
    }
  }
`);
```

### 3. 错误处理

```typescript
try {
  const result = await gql.executeQuery(query, variables);

  if (result.errors && result.errors.length > 0) {
    console.error('GraphQL 执行错误:', result.errors);
    return;
  }

  console.log('查询成功:', result.data);
} catch (error) {
  console.error('查询异常:', error);
}
```

## API 参考

### GraphQLService

#### 构造函数

```typescript
import { GraphQLService } from '@/query/graphql';

const service = new GraphQLService(store);
```

#### 方法

- `initialize(): Promise<void>` - 初始化服务
- `getSchema(): Promise<string>` - 获取 GraphQL SDL
- `getSchemaStatistics(): Promise<SchemaStatistics>` - 获取统计信息
- `executeQuery(query: string, variables?: any): Promise<any>` - 执行查询
- `validateQuery(query: string): Promise<GraphQLError[]>` - 验证查询
- `calculateQueryComplexity(query: string): Promise<number>` - 计算复杂度
- `regenerateSchema(): Promise<void>` - 重新生成 Schema
- `dispose(): void` - 清理资源

### 便捷函数

```typescript
import { graphql, discoverSchema, buildSchema } from '@/query/graphql';

// 创建服务
const gql = graphql(store);

// 仅发现 Schema
const entityTypes = await discoverSchema(store, config);

// 从实体类型构建 Schema
const schema = await buildSchema(store, entityTypes, resolverOptions);
```

### 配置选项

#### SchemaGenerationConfig

```typescript
interface SchemaGenerationConfig {
  maxSampleSize?: number; // 最大样本数量
  minEntityCount?: number; // 最小实体数量
  typeMapping?: Record<string, string>; // 自定义类型映射
  fieldNaming?: 'camelCase' | 'snake_case' | 'preserve'; // 字段命名
  includeReverseRelations?: boolean; // 包含反向关系
  maxDepth?: number; // 最大遍历深度
  excludeTypes?: string[]; // 排除的类型
  includeTypes?: string[]; // 仅包含的类型
  excludePredicates?: string[]; // 排除的谓词
}
```

#### ResolverGenerationOptions

```typescript
interface ResolverGenerationOptions {
  enablePagination?: boolean; // 启用分页
  enableFiltering?: boolean; // 启用过滤
  enableSorting?: boolean; // 启用排序
  enableAggregation?: boolean; // 启用聚合
  maxQueryDepth?: number; // 最大查询深度
  maxQueryComplexity?: number; // 最大查询复杂度
}
```

## 故障排除

### 常见问题

1. **没有生成类型**
   - 检查数据中是否有明确的类型声明（TYPE 谓词）
   - 调整 `minEntityCount` 参数
   - 验证数据格式是否正确

2. **字段类型不正确**
   - 确保属性值的数据类型一致
   - 使用适当的类型映射配置

3. **关系未被发现**
   - 验证关系三元组是否正确建立
   - 检查目标实体是否存在类型声明

4. **查询执行失败**
   - 验证查询语法
   - 检查字段名是否存在
   - 确认参数类型匹配

### 调试技巧

```typescript
// 启用详细日志
const gql = createGraphQLService(db.store, {
  // 使用较小的采样大小进行调试
  maxSampleSize: 100,
  minEntityCount: 1,
});

// 检查发现的实体类型
const entityTypes = await discoverSchema(db.store);
console.log(
  '发现的类型:',
  entityTypes.map((t) => t.typeName),
);

// 检查生成的 Schema
const schema = await gql.getSchema();
console.log('生成的 Schema:', schema);

// 验证查询
const errors = await gql.validateQuery(yourQuery);
if (errors.length > 0) {
  console.log('查询验证错误:', errors);
}
```

## 总结

GraphQL 动态 Schema 生成器为 SynapseDB 提供了强大的查询接口，能够：

- 自动分析知识图谱结构
- 生成类型安全的 GraphQL Schema
- 支持复杂的嵌套查询
- 提供分页、过滤、排序功能
- 实现动态 Schema 更新
- 优化查询性能

通过合理配置和使用，可以快速构建功能完整的 GraphQL API，为知识图谱数据提供直观的查询界面。
