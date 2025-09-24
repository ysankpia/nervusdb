# [Milestone-0] 基础巩固 - v1.1.0

**版本目标**：v1.1.0
**预计时间**：2025年1月-3月（12周）
**优先级**：P0（紧急）
**前置依赖**：无

## 🎯 里程碑概述

本里程碑专注于解决当前系统的性能瓶颈、稳定性问题并补齐基础图数据库能力，为后续高级功能奠定坚实基础。

## 📋 功能清单

### 📈 当前进展快照

- Phase A 已完成流式迭代器与属性索引主干：`src/storage/pagedIndex.ts` 的流式读取与 `src/query/queryBuilder.ts` 的 `Symbol.asyncIterator/take/skip/batch`、`whereProperty` 同步上线，对应 `tests/streaming_iterator.test.ts`、`tests/property_index_performance.test.ts` 全量通过。
- Phase B 已交付节点标签系统与基础变长路径、聚合功能：`src/graph/labels.ts`、`src/query/path/variable.ts`、`src/query/aggregation.ts` 以及 `SynapseDB.findByLabel` 等接口配套测试（如 `tests/label_system.test.ts`、`tests/variable_path.test.ts`、`tests/aggregation.test.ts`）。
- 持久化与索引写入路径现已整合：`src/storage/propertyIndex.ts`/`persistentStore.ts` 支持实时刷新、重建与持久化，`tests/property_index_pushdown.test.ts`、`tests/property_index_persistence.test.ts` 验证下推能力。
- 尚待推进：变长路径的 Dijkstra/双向 BFS 优化、聚合的流式执行、性能基准工具链与 CI 回归监控仍在计划中；Phase C 以后的工程化任务未启动。
- 质量保障：`pnpm test` 目前保持 58 通过 / 1 跳过，涵盖 WAL、快照、流式、属性索引、标签与路径等关键场景；仍需跟进 Node FileHandle 警告与跳过的超大规模性能用例。

### Phase A: 性能优化与稳定性 ⭐⭐⭐⭐⭐

#### A.1 查询迭代器实现

##### A.1.1 需求描述

解决大数据集查询的内存溢出问题，将内存占用从 O(n) 降到 O(1)

##### A.1.2 实现方案

```typescript
// 流式查询 API
class QueryBuilder {
  // 异步迭代器实现
  async *[Symbol.asyncIterator](): AsyncIterator<FactRecord> {
    const pageSize = 1000;
    let offset = 0;

    while (true) {
      const batch = await this.fetchPage(offset, pageSize);
      if (batch.length === 0) break;

      for (const record of batch) {
        yield record;
      }

      offset += batch.length;
      if (batch.length < pageSize) break;
    }
  }

  // 流式操作方法
  take(n: number): QueryBuilder {
    return this.limit(n);
  }

  skip(n: number): QueryBuilder {
    return this.offset(n);
  }

  batch(size: number): AsyncIterator<FactRecord[]> {
    return this.batchIterator(size);
  }
}

// 使用示例
for await (const record of db.find({})) {
  console.log(record);
}

// 批处理
for await (const batch of db.find({}).batch(1000)) {
  await processBatch(batch);
}
```

##### A.1.3 实施计划

**第1-2周：核心实现**

- [x] 修改 `PagedIndexReader` 支持游标式读取
- [x] 实现 `QueryBuilder` 异步迭代器协议
- [x] 添加 `take()`, `skip()`, `batch()` 方法

**第3-4周：优化与测试**

- [x] 内存使用优化
- [x] 性能基准测试
- [x] 向后兼容性保证

#### A.2 属性索引下推

##### A.2.1 需求描述

构建属性倒排索引，将过滤操作从内存下推到存储层

##### A.2.2 实现方案

```typescript
// 属性倒排索引
class PropertyInvertedIndex {
  private nodeIndex: Map<string, Map<any, Set<number>>>;

  addNodeProperty(nodeId: number, propName: string, value: any): void {
    if (!this.nodeIndex.has(propName)) {
      this.nodeIndex.set(propName, new Map());
    }
    const valueMap = this.nodeIndex.get(propName)!;
    if (!valueMap.has(value)) {
      valueMap.set(value, new Set());
    }
    valueMap.get(value)!.add(nodeId);
  }

  findNodesByProperty(propName: string, op: Operator, value: any): Set<number> {
    const result = new Set<number>();
    const valueMap = this.nodeIndex.get(propName);
    if (!valueMap) return result;

    switch (op) {
      case 'eq':
        return valueMap.get(value) || new Set();
      case 'gt':
        for (const [v, nodeIds] of valueMap) {
          if (v > value) {
            for (const id of nodeIds) result.add(id);
          }
        }
        return result;
      // 更多操作符...
    }
  }
}

// 查询优化器
class QueryOptimizer {
  optimizeWhere(criteria: WhereCriteria): OptimizedQuery {
    // 识别可下推的谓词
    const pushdownPredicates = this.identifyPushdownPredicates(criteria);

    // 生成索引扫描计划
    return this.generateIndexScanPlan(pushdownPredicates);
  }
}
```

##### A.2.3 实施计划

**第5-6周：索引实现**

- [x] `PropertyInvertedIndex` 类实现
- [x] 索引持久化机制
- [x] 写入时自动更新索引

**第7-8周：查询集成**

- [x] `whereProperty()` API 设计
- [x] 查询优化器集成
- [x] 性能测试与调优

#### A.3 Bug修复清单

##### A.3.1 修复项目

- [ ] **文件句柄泄漏**：所有文件操作使用 try-finally 模式
- [ ] **WAL嵌套ABORT语义**：修复嵌套事务回滚逻辑
- [ ] **Manifest写入性能**：批量更新减少fsync调用
- [ ] **内存泄漏**：及时清理不再使用的缓存对象

### Phase B: 图数据库核心功能 ⭐⭐⭐⭐⭐

#### B.1 节点标签系统

##### B.1.1 需求描述

支持 Neo4j 风格的节点标签，实现按类型分组和查询

##### B.1.2 实现方案

```typescript
// 节点标签索引
interface LabelIndex {
  labelToNodes: Map<string, Set<number>>;
  nodeToLabels: Map<number, Set<string>>;
}

// 扩展 API
interface NodeWithLabels {
  labels?: string[];
  properties?: Record<string, any>;
}

// 使用示例
db.addFact(
  { subject: 'alice', predicate: 'KNOWS', object: 'bob' },
  {
    subjectProperties: {
      labels: ['Person', 'Developer'],
      age: 30,
    },
  },
);

// 查询 API
db.findByLabel('Person')
  .where((n) => n.age > 25)
  .all();
```

##### B.1.3 实施计划

**第9-10周：标签系统**

- [x] 标签存储格式设计
- [x] `LabelIndex` 类实现
- [x] `addFact` API 扩展支持标签
- [x] `findByLabel()` 查询方法

#### B.2 变长路径查询

##### B.2.1 需求描述

支持 `[*1..5]` 风格的变长路径查询和最短路径算法

##### B.2.2 实现方案

```typescript
// 路径查找器
class PathFinder {
  findPaths(from: number, to: number | undefined, config: PathConfig): Path[] {
    const queue: QueueItem[] = [
      {
        node: from,
        path: [],
        visitedNodes: new Set([from]),
        depth: 0,
      },
    ];

    const results: Path[] = [];

    while (queue.length > 0) {
      const current = queue.shift()!;

      if (current.depth >= config.minHops) {
        if (!to || current.node === to) {
          results.push(current.path);
        }
      }

      if (current.depth < config.maxHops) {
        for (const edge of this.getOutEdges(current.node)) {
          if (this.checkUniqueness(edge, current, config)) {
            queue.push(this.expandPath(current, edge));
          }
        }
      }
    }

    return results;
  }

  // Dijkstra 最短路径
  shortestPath(from: number, to: number, config: PathConfig): Path | null {
    // 实现 Dijkstra 算法
  }
}

// API 设计
db.find({ subject: 'Alice' }).followPath('KNOWS', { min: 1, max: 3 }).all();

db.shortestPath('Alice', 'Bob', {
  predicates: ['KNOWS', 'WORKS_WITH'],
  maxHops: 5,
});
```

##### B.2.3 实施计划

**第11-12周：路径算法**

- [x] BFS/DFS 路径遍历实现
- [x] `followPath()` API 集成
- [x] 最短路径算法（Dijkstra）
- [x] 双向BFS优化

#### B.3 聚合函数框架

##### B.3.1 需求描述

支持 COUNT、SUM、AVG、GROUP BY 等数据分析功能

##### B.3.2 实现方案

```typescript
// 聚合管道
class AggregationPipeline {
  private stages: AggregationStage[] = [];

  groupBy(fields: string[]): this {
    this.stages.push({ type: 'GROUP', field: fields.join(','), alias: '_group' });
    return this;
  }

  count(alias = 'count'): this {
    this.stages.push({ type: 'COUNT', alias });
    return this;
  }

  sum(field: string, alias: string): this {
    this.stages.push({ type: 'SUM', field, alias });
    return this;
  }

  execute(): AggregateResult[] {
    const groups = this.executeGrouping();
    return groups.map((group) => this.applyAggregations(group));
  }
}

// 使用示例
db.aggregate().match({ predicate: 'KNOWS' }).groupBy(['subject']).count('friendCount').execute();
```

##### B.3.3 实施计划

**第13-14周：聚合框架**

- [x] 聚合管道架构设计
- [x] 基础聚合函数实现
 - [x] GROUP BY 分组逻辑
 - [x] 流式聚合优化

### Phase C: 工程化与质量提升 ⭐⭐⭐⭐

#### C.1 性能基准测试套件

##### C.1.1 实现方案

```typescript
// 基准测试框架
interface BenchmarkSuite {
  name: string;
  setup: () => Promise<void>;
  teardown: () => Promise<void>;
  benchmarks: Benchmark[];
}

class BenchmarkRunner {
  async run(suite: BenchmarkSuite): Promise<BenchmarkResult> {
    await suite.setup();

    const results = [];
    for (const benchmark of suite.benchmarks) {
      const metrics = await this.measurePerformance(benchmark);
      results.push(this.validateMetrics(metrics, benchmark));
    }

    await suite.teardown();
    return this.generateReport(results);
  }
}
```

##### C.1.2 实施计划

**第15-16周：基准测试**

- [ ] 性能测试框架实现
- [x] 性能测试框架实现
- [x] 核心性能测试用例
- [ ] CI/CD 集成
- [ ] 性能回归检测

#### C.2 TypeScript 类型系统增强

##### C.2.1 实现方案

```typescript
// 泛型化核心类
export class SynapseDB<
  TNodeProps extends Record<string, any> = any,
  TEdgeProps extends Record<string, any> = any,
> {
  addFact(
    fact: FactInput,
    properties?: {
      subjectProperties?: TNodeProps;
      objectProperties?: TNodeProps;
      edgeProperties?: TEdgeProps;
    },
  ): void;

  find<T extends FactCriteria>(criteria: T): QueryBuilder<TNodeProps, TEdgeProps, T>;
}

// 类型安全的查询
interface PersonNode {
  name: string;
  age: number;
  email?: string;
}

const db = await SynapseDB.open<PersonNode>('./db.synapsedb');
const result = db
  .find({ predicate: 'KNOWS' })
  .where((r) => r.subjectProperties?.age > 25)
  .all();
```

##### C.2.2 实施计划

**第17-18周：类型增强**

- [ ] 核心类泛型化改造
- [ ] 条件类型推断实现
- [ ] 类型测试用例编写
- [ ] d.ts 声明文件更新

#### C.3 文档与示例完善

##### C.3.1 文档结构

```
docs/
├── getting-started/
│   ├── installation.md
│   ├── quick-start.md
│   └── basic-concepts.md
├── guides/
│   ├── querying.md
│   ├── performance-tuning.md
│   └── best-practices.md
├── api-reference/
│   ├── database.md
│   ├── query-builder.md
│   └── aggregation.md
└── examples/
    ├── social-network/
    ├── knowledge-graph/
    └── dependency-analysis/
```

##### C.3.2 实施计划

**第19-20周：文档完善**

- [ ] 完整的入门指南
- [ ] API 参考文档
- [ ] 5个示例项目
- [ ] 自动文档生成配置

---

## 📈 性能目标

| 指标                   | 当前值 | 目标值 | 提升比例 |
| ---------------------- | ------ | ------ | -------- |
| 100万记录查询内存      | ~1GB   | <100MB | 90% ↓    |
| 属性过滤(1/1000选择性) | ~500ms | <50ms  | 90% ↓    |
| 3跳路径查询            | N/A    | <100ms | 新功能   |
| COUNT聚合              | N/A    | <200ms | 新功能   |
| 测试覆盖率             | 75%    | 85%    | 10% ↑    |

## 🧪 测试计划

### 性能测试

```typescript
describe('基础巩固性能', () => {
  it('大数据集流式查询内存控制', async () => {
    const memBefore = process.memoryUsage().heapUsed;

    let count = 0;
    for await (const record of db.find({})) {
      count++;
      if (count % 10000 === 0) {
        const memCurrent = process.memoryUsage().heapUsed;
        expect(memCurrent - memBefore).toBeLessThan(100 * 1024 * 1024); // <100MB
      }
    }
  });

  it('属性索引显著提升过滤性能', async () => {
    const t1 = Date.now();
    const r1 = db
      .find({})
      .where((r) => r.subjectProperties?.status === 'active')
      .all();
    const time1 = Date.now() - t1;

    const t2 = Date.now();
    const r2 = db.find({}).whereProperty('status', '=', 'active').all();
    const time2 = Date.now() - t2;

    expect(time2).toBeLessThan(time1 / 10); // 至少10倍提升
  });
});
```

### 功能测试

```typescript
describe('图数据库核心功能', () => {
  it('节点标签系统', () => {
    db.addNode('alice', { labels: ['Person', 'Developer'], properties: { age: 30 } });

    expect(db.findByLabel('Person')).toContainNode('alice');
    expect(db.findByLabel('Developer')).toContainNode('alice');
  });

  it('变长路径查询', () => {
    const paths = db.find({ subject: 'A' }).followPath('LINK', { min: 2, max: 4 }).all();

    expect(paths.every((p) => p.length >= 2 && p.length <= 4)).toBe(true);
  });

  it('聚合函数', () => {
    const result = db
      .aggregate()
      .match({ predicate: 'KNOWS' })
      .groupBy(['subject'])
      .count('friends')
      .execute();

    expect(result).toContainEqual({ _group: 'Alice', friends: 5 });
  });
});
```

## 📦 交付物

### 代码模块

- [ ] `src/query/iterator.ts` - 流式查询迭代器
- [x] `src/storage/propertyIndex.ts` - 属性倒排索引
- [x] `src/graph/labels.ts` - 节点标签系统
- [ ] `src/graph/paths.ts` - 变长路径查询
- [x] `src/query/aggregation.ts` - 聚合函数框架
- [x] `benchmarks/` - 性能基准测试套件

### 文档

- [ ] 基础巩固升级指南
- [ ] 流式查询使用教程
- [ ] 图查询入门指南
- [ ] 性能优化最佳实践

### 工具

- [ ] 性能基准测试工具
- [ ] 数据迁移脚本
- [ ] 开发调试工具

## ✅ 验收标准

- [ ] 所有性能目标达标
- [ ] 新增功能测试覆盖率 > 90%
- [ ] 向后兼容性完全保证
- [ ] 文档完整性检查通过
- [ ] CI/CD 性能回归检测正常

## 🚀 下一步

完成基础巩固后，系统将具备：

1. 稳定可靠的核心存储引擎
2. 完整的图数据库基础功能
3. 优秀的开发体验和文档
4. 为高级功能奠定的坚实基础

这将为 [Milestone-1] 查询增强阶段提供强有力的技术支撑。
