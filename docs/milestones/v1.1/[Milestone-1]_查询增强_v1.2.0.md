# [Milestone-1] 查询增强 - v1.2.0

**版本目标**：v1.2.0
**预计时间**：2025年3月-5月（12周）
**优先级**：P1（高优先级）
**前置依赖**：Phase A-C 完成

## 🎯 里程碑概述

本里程碑专注于实现标准图数据库的核心查询能力，使 SynapseDB 具备与 Neo4j 等主流图数据库相当的查询表达力。

## 📌 当前进展快照（2025-09-24）

- ✅ 变长路径能力：`variablePath()`、唯一性约束、与模式匹配编排集成；提供加权 `shortestPathWeighted()`
- ✅ 聚合管道：`groupBy/count/sum/avg/min/max/orderBy/limit`，提供内存优化的流式聚合 `executeStreaming()`
- ✅ UNION/UNION ALL：`queryBuilder.union()` 与 `unionAll()` 已实现
- 🚧 模式匹配：提供编程式 `PatternBuilder`（`.node()/.edge()/.variable()/.return()/.execute()`）；文本解析/查询计划器未落地
- 🧪 测试现状：相关测试均通过（见 `tests/variable_path*.test.ts`、`tests/aggregation_streaming.test.ts`、`tests/pattern_match_advanced.test.ts`、`tests/streaming_iterator.test.ts`）

## 📋 功能清单

### 1. 模式匹配查询 ⭐⭐⭐⭐⭐

#### 1.1 需求描述

实现类似 Neo4j 的模式匹配语法：`(a)-[:KNOWS]->(b)`

#### 1.2 设计方案

```typescript
// 模式匹配 API 设计
interface PatternBuilder {
  // 节点模式
  node(alias?: string, labels?: string[], props?: object): this;

  // 边模式
  edge(direction: '->' | '<-' | '-', type?: string, alias?: string): this;

  // 路径模式
  path(alias: string): this;

  // 变长模式
  variable(min: number, max: number): this;

  // WHERE 条件
  where(condition: string | ((ctx: MatchContext) => boolean)): this;

  // 返回子句
  return(items: string[]): this;

  // 执行查询
  execute(): Promise<PatternResult[]>;
}

// 使用示例
const result = await db
  .match()
  .node('person', ['Person'], { age: { $gt: 25 } })
  .edge('->', 'KNOWS', 'rel')
  .node('friend', ['Person'])
  .where('rel.since > date("2020-01-01")')
  .return(['person.name', 'friend.name', 'rel.since'])
  .execute();
```

#### 1.3 实现计划

**第1-2周：模式解析器**

- [ ] 设计 AST 节点结构
- [ ] 实现模式解析器（PEG.js）
- [ ] 语法错误处理和提示

**第3-4周：查询计划器**

- [ ] 实现查询计划生成
- [ ] 基础成本估算模型
- [ ] 索引选择优化

**第5-6周：执行引擎**

- [ ] 火山模型执行器
- [x] 节点/边匹配算法（编程式 PatternBuilder 已实现）
- [x] 结果集构建（`.return().execute()` 已实现）

#### 1.4 验收标准

- [ ] 支持基础模式匹配语法
- [ ] 处理复杂的多节点模式
- [ ] 查询性能与手工 follow 链相当

---

### 2. 变长路径查询 ⭐⭐⭐⭐⭐

#### 2.1 需求描述

支持 `[*1..5]` 语法的变长路径遍历

#### 2.2 设计方案

```typescript
// 变长路径 API
interface VariablePathQuery {
  // 基础变长路径
  variablePath(
    relation: string,
    options: {
      min?: number;
      max?: number;
      uniqueness?: 'NODE' | 'EDGE' | 'NONE';
    },
  ): this;

  // 最短路径
  shortestPath(target: string | FactCriteria): PathResult[];

  // 所有路径
  allPaths(target: string | FactCriteria, maxDepth: number): PathResult[];
}

// 路径结果结构
interface PathResult {
  startNode: FactRecord;
  endNode: FactRecord;
  path: EdgeResult[];
  length: number;
  weight?: number;
}

// 使用示例
const paths = db
  .find({ subject: 'Alice' })
  .variablePath('KNOWS', { min: 2, max: 4, uniqueness: 'NODE' })
  .where((path) => path.length <= 3)
  .all();

const shortestPath = db.shortestPath('Alice', 'Bob');
```

#### 2.3 算法实现

**BFS 变长路径算法**

```typescript
class VariablePathFinder {
  findPaths(
    start: number,
    end: number | undefined,
    predicate: string,
    options: PathOptions,
  ): Path[] {
    const queue: QueueItem[] = [
      {
        node: start,
        path: [],
        visited: new Set([start]),
        depth: 0,
      },
    ];

    const results: Path[] = [];

    while (queue.length > 0) {
      const current = queue.shift()!;

      // 检查是否达到最小深度
      if (current.depth >= (options.min || 1)) {
        if (!end || current.node === end) {
          results.push(current.path);
        }
      }

      // 继续扩展
      if (current.depth < (options.max || 5)) {
        const neighbors = this.getNeighbors(current.node, predicate);

        for (const neighbor of neighbors) {
          // 唯一性检查
          if (options.uniqueness === 'NODE' && current.visited.has(neighbor.target)) {
            continue;
          }

          queue.push({
            node: neighbor.target,
            path: [...current.path, neighbor],
            visited: new Set([...current.visited, neighbor.target]),
            depth: current.depth + 1,
          });
        }
      }
    }

    return results;
  }
}
```

#### 2.4 实现计划

**第7-8周：基础算法**

- [x] BFS 路径遍历实现（`VariablePathBuilder`）
- [x] 唯一性约束处理（`uniqueness: 'NODE' | 'EDGE' | 'NONE'`）
- [x] 路径结果格式化（`all()` 返回结构）

**第9-10周：优化算法**

- [ ] 双向 BFS 优化
- [x] Dijkstra 最短路径（支持边权 `weight`）
- [ ] A\* 启发式搜索

**第11-12周：集成与优化**

- [x] 与模式匹配集成（变长路径在 PatternBuilder 中可用）
- [ ] 性能优化和调试
- [ ] 内存使用优化

#### 2.5 验收标准

- [ ] 支持所有变长路径语法
- [ ] 最短路径算法正确性
- [ ] 10万节点图上3跳查询 < 100ms

---

### 3. 聚合函数与分组 ⭐⭐⭐⭐

#### 3.1 需求描述

实现 SQL 风格的聚合和分组功能

#### 3.2 功能设计

```typescript
// 聚合查询 API
interface AggregationQuery {
  // 分组
  groupBy(...fields: string[]): this;

  // 聚合函数
  count(alias?: string): this;
  sum(field: string, alias?: string): this;
  avg(field: string, alias?: string): this;
  min(field: string, alias?: string): this;
  max(field: string, alias?: string);

  // 排序
  orderBy(field: string, direction: 'ASC' | 'DESC'): this;

  // 限制
  limit(count: number): this;

  // 执行
  execute(): AggregateResult[];
}

// 使用示例
const stats = db
  .aggregate()
  .match({ predicate: 'KNOWS' })
  .groupBy('subject')
  .count('friendCount')
  .avg('edgeProperties.strength', 'avgStrength')
  .orderBy('friendCount', 'DESC')
  .limit(10)
  .execute();
```

#### 3.3 实现计划

**第13-14周：聚合框架**

- [x] 聚合管道设计（`AggregationPipeline`）
- [x] 基础聚合函数实现（COUNT/SUM/AVG/MIN/MAX）
- [x] 分组逻辑实现（`groupBy()` 多字段）

**第15-16周：优化与集成**

- [x] 流式聚合优化（增量状态、部分排序 `partialSort`）
- [x] 内存使用控制（不保留完整记录数组）
- [x] 与查询引擎集成（`match()/matchStream()`）

#### 3.4 验收标准

- [ ] 支持所有基础聚合函数
- [ ] 多字段分组正确性
- [ ] 100万记录聚合 < 500ms

---

### 4. 联合查询与子查询 ⭐⭐⭐

#### 4.1 UNION 查询

```typescript
// UNION 支持
interface UnionQuery {
  union(other: QueryBuilder): this;
  unionAll(other: QueryBuilder): this;
}

// 使用示例
const result = db
  .find({ predicate: 'KNOWS' })
  .union(db.find({ predicate: 'WORKS_WITH' }))
  .all();
```

#### 4.2 子查询支持

```typescript
// 子查询类型
type SubqueryType = 'EXISTS' | 'NOT_EXISTS' | 'IN' | 'NOT_IN';

// 子查询 API
interface SubqueryBuilder {
  exists(subquery: QueryBuilder): this;
  notExists(subquery: QueryBuilder): this;
  in(field: string, subquery: QueryBuilder): this;
  notIn(field: string, subquery: QueryBuilder): this;
}

// 使用示例
const result = db
  .find({ predicate: 'WORKS_AT' })
  .where((ctx) => ctx.exists(db.find({ subject: ctx.subject, predicate: 'MANAGES' })))
  .all();
```

#### 4.3 实现计划

**第17-18周：基础实现**

- [x] UNION 查询实现（`union/unionAll`）
- [ ] 基础子查询框架
- [ ] EXISTS/NOT EXISTS

**第19-20周：完善功能**

- [ ] IN/NOT IN 子查询
- [ ] 相关子查询支持
- [ ] 性能优化

#### 4.4 验收标准

- [ ] UNION 查询去重正确
- [ ] 子查询执行正确
- [ ] 复杂嵌套查询支持

---

## 📈 性能目标

| 功能         | 数据规模  | 目标性能 | 内存限制 |
| ------------ | --------- | -------- | -------- |
| 简单模式匹配 | 100万节点 | < 50ms   | < 200MB  |
| 复杂模式匹配 | 100万节点 | < 200ms  | < 500MB  |
| 3跳变长路径  | 10万节点  | < 100ms  | < 100MB  |
| 5跳变长路径  | 10万节点  | < 500ms  | < 200MB  |
| 聚合查询     | 100万记录 | < 300ms  | < 300MB  |
| UNION查询    | 50万记录  | < 100ms  | < 200MB  |

## 🧪 测试计划

### 功能测试

```typescript
describe('模式匹配查询', () => {
  it('支持基础节点-边-节点模式', () => {
    const result = db
      .match()
      .node('a', ['Person'])
      .edge('->', 'KNOWS')
      .node('b', ['Person'])
      .execute();

    expect(result).toHaveLength(expectedCount);
  });

  it('支持复杂多节点模式', () => {
    const result = db
      .match()
      .node('a', ['Person'])
      .edge('->', 'KNOWS')
      .node('b', ['Person'])
      .edge('->', 'WORKS_AT')
      .node('c', ['Company'])
      .where('a.age > 25 AND c.size > 100')
      .execute();

    expect(result).toBeDefined();
  });
});

describe('变长路径查询', () => {
  it('找到正确的变长路径', () => {
    const paths = db.find({ subject: 'Alice' }).variablePath('KNOWS', { min: 2, max: 4 }).all();

    expect(paths.every((p) => p.length >= 2 && p.length <= 4)).toBe(true);
  });

  it('最短路径算法正确', () => {
    const path = db.shortestPath('Alice', 'Bob');
    expect(path.length).toBe(expectedLength);
  });
});
```

### 性能测试

```typescript
describe('查询性能', () => {
  it('大规模模式匹配性能', async () => {
    // 创建100万节点图
    const startTime = Date.now();

    const result = db
      .match()
      .node('a', ['Person'])
      .edge('->', 'KNOWS')
      .node('b', ['Person'])
      .execute();

    const duration = Date.now() - startTime;
    expect(duration).toBeLessThan(50);
  });
});
```

## 📦 交付物

### 代码模块

- [ ] `src/query/pattern/` - 模式匹配模块
- [ ] `src/query/path/` - 路径查询模块
- [ ] `src/query/aggregation/` - 聚合查询模块
- [ ] `src/query/union/` - 联合查询模块

### 文档

- [ ] 查询语言参考手册
- [ ] 模式匹配教程
- [ ] 性能调优指南
- [ ] 迁移指南（从链式API）

### 测试

- [ ] 功能测试覆盖率 > 90%
- [ ] 性能基准测试
- [ ] 压力测试报告

## ✅ 验收标准

- [ ] 所有功能测试通过
- [ ] 性能指标达标
- [ ] 向后兼容性保证
- [ ] 文档完整性检查
- [ ] 内存泄漏测试通过

## 🚀 下一步

完成查询增强后，进入 [Milestone-2] 标准兼容阶段，实现 Cypher 和 GraphQL 支持。
