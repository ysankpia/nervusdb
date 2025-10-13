# GraphQL 使用指南

## 接口说明

NervusDB 提供 GraphQL 端点（需在应用层封装），基本 schema 如下：

```graphql
schema {
  query: Query
}

type Query {
  synapse: SynapseQuery!
}

type SynapseQuery {
  find(subject: ID, predicate: String, object: ID, anchor: Anchor): [Fact!]!
}

type Fact {
  subject: ID!
  predicate: String!
  object: ID!
  subjectProperties: JSON
  edgeProperties: JSON
  objectProperties: JSON
}
```

## 查询示例

```graphql
query Friends($id: ID!) {
  synapse {
    find(subject: $id, predicate: "FRIEND_OF") {
      object
      edgeProperties
      follow(predicate: "WORKS_AT") {
        object
      }
    }
  }
}
```

### 变量

```graphql
query TeamMembers($team: ID!) {
  synapse {
    find(predicate: "WORKS_AT", object: $team) {
      subject
    }
  }
}
```

### 聚合（封装）

> GraphQL 层可封装 QueryBuilder 聚合结果，例如：

```graphql
query TeamStats($team: ID!) {
  synapse {
    teamStats(team: $team) {
      memberCount
      avgStrength
    }
  }
}
```

服务器中实现 `teamStats` 调用聚合 API。

## 注意

- GraphQL 层仅是语法糖，本质调用 QueryBuilder
- 大结果建议返回分页或调用 Streaming API 后封装
- 可以结合 Apollo/Helix 等服务框架

## 故障排查

| 症状       | 解决                             |
| ---------- | -------------------------------- |
| 字段未定义 | 更新 schema，确保 resolver 实现  |
| 结果为空   | 检查参数或底层数据               |
| 性能问题   | 缓存热点查询、优化分页、调整索引 |

## 延伸阅读

- [docs/教学文档/教程-03-查询与链式联想.md](../教学文档/教程-03-查询与链式联想.md)
- [docs/使用示例/03-查询与联想-示例.md](03-查询与联想-示例.md)
