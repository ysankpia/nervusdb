# SynapseDB 项目发展路线图

> 本文档详细规划了 SynapseDB 从当前的三元组知识库向标准图数据库演进的技术路线图。
>
> 最后更新：2025-01-24
> 当前版本：v1.0.0
> 目标版本：v2.0.0

## 目录

1. [项目愿景](#项目愿景)
2. [市场分析与差异化定位](#市场分析与差异化定位)
3. [技术现状评估](#技术现状评估)
4. [发展阶段规划](#发展阶段规划)
5. [详细实施方案](#详细实施方案)
6. [技术架构演进](#技术架构演进)
7. [API 设计规范](#api-设计规范)
8. [性能基准目标](#性能基准目标)
9. [风险评估与缓解](#风险评估与缓解)
10. [社区建设计划](#社区建设计划)
11. [长期发展展望](#长期发展展望)

---

## 项目愿景

### 核心定位

将 SynapseDB 打造成为：

- **轻量级**：单文件、零依赖的嵌入式图数据库
- **标准兼容**：支持 openCypher/Gremlin 标准查询语言
- **高性能**：基于六维索引的毫秒级查询响应
- **易用性**：TypeScript 原生，完善的类型支持
- **可靠性**：WAL、MVCC、崩溃恢复机制完备

### 目标用户

- **应用开发者**：需要嵌入式图存储的应用
- **知识图谱**：构建领域知识图谱的团队
- **AI/LLM 应用**：代码理解、知识推理场景
- **研究人员**：图算法研究与实验

---

## 市场分析与差异化定位

### 主流图数据库对比

| 数据库             | 类型     | 部署模式    | 主要语言 | 存储大小 | 使用场景       | 许可证        |
| ------------------ | -------- | ----------- | -------- | -------- | -------------- | ------------- |
| **Neo4j**          | 原生图   | 服务器/集群 | Java     | GB-TB级  | 企业级图分析   | GPL/商业      |
| **TigerGraph**     | 原生图   | 分布式集群  | C++      | TB-PB级  | 大规模实时分析 | 商业          |
| **ArangoDB**       | 多模型   | 服务器/集群 | C++      | GB-TB级  | 多模型数据库   | Apache 2.0    |
| **Amazon Neptune** | 托管服务 | 云服务      | -        | TB级     | AWS生态        | 商业          |
| **JanusGraph**     | 分布式   | 集群        | Java     | TB-PB级  | 大规模图处理   | Apache 2.0    |
| **DGraph**         | 原生图   | 分布式      | Go       | GB-TB级  | GraphQL原生    | Apache 2.0    |
| **RedisGraph**     | 内存图   | 服务器      | C        | GB级     | 实时查询       | Redis License |

### 嵌入式/轻量级选择对比

| 数据库            | 语言       | 特点        | 限制                   |
| ----------------- | ---------- | ----------- | ---------------------- |
| **SQLite + FTS5** | C          | 成熟稳定    | 非原生图，需要自建图层 |
| **LevelGraph**    | JavaScript | 基于LevelDB | 性能有限，项目不活跃   |
| **Cayley**        | Go         | 支持多后端  | 需要外部存储           |
| **GunDB**         | JavaScript | P2P分布式   | 复杂度高，不适合嵌入   |

### SynapseDB 独特定位

#### 核心差异化优势

```typescript
const synapseDBAdvantages = {
  // 1. 真正的嵌入式
  deployment: {
    type: 'embedded', // ✅ 不需要服务器
    size: '< 1MB', // ✅ 极小的运行时
    dependencies: 'zero', // ✅ 零依赖
    file: 'single-file', // ✅ 单文件数据库
  },

  // 2. TypeScript 原生
  language: {
    runtime: 'TypeScript/JavaScript', // ✅ 前后端通用
    types: 'full-typed', // ✅ 完整类型支持
    ecosystem: 'npm', // ✅ npm 生态
    browser: 'compatible', // ✅ 浏览器兼容（未来）
  },

  // 3. 知识图谱优化
  specialization: {
    model: 'SPO-triples', // ✅ 三元组原生
    index: '6-dimensional', // ✅ 六维索引
    query: 'chain-associative', // ✅ 链式联想
    useCase: 'code-knowledge', // ✅ 代码知识图谱
  },

  // 4. 开发者友好
  dx: {
    setup: 'npm install', // ✅ 一行安装
    api: 'intuitive', // ✅ 直观API
    learning: '< 30min', // ✅ 快速上手
    debugging: 'transparent', // ✅ 透明调试
  },
};
```

### 市场空白分析

| 痛点               | 现有方案问题             | SynapseDB 方案               |
| ------------------ | ------------------------ | ---------------------------- |
| **部署复杂**       | Neo4j需要JVM，配置复杂   | `npm install synapsedb` 即可 |
| **资源占用**       | 最小Neo4j也需要512MB+    | 运行时 < 10MB                |
| **学习成本**       | Cypher/Gremlin学习曲线陡 | 链式API，30分钟上手          |
| **前端集成**       | 需要后端API服务器        | 可直接在浏览器运行           |
| **小规模数据**     | 大炮打蚊子，过度设计     | 专为中小规模优化             |
| **TypeScript生态** | 缺少原生TS图数据库       | 100% TypeScript              |

### 独特应用场景

1. **Electron/Tauri 桌面应用**

   ```typescript
   // 本地知识库应用
   const kb = await SynapseDB.open('./my-knowledge.db');
   // 无需启动数据库服务器！
   ```

2. **VS Code 扩展**

   ```typescript
   // 代码分析插件
   const codeGraph = await SynapseDB.open(path.join(context.extensionPath, 'code-graph.db'));
   ```

3. **CLI 工具**

   ```typescript
   // 依赖分析工具
   #!/usr/bin/env node
   import { SynapseDB } from 'synapsedb';
   const db = await SynapseDB.open('./deps.db');
   ```

4. **边缘计算/IoT**

   ```typescript
   // 树莓派上的图数据库
   const sensorGraph = await SynapseDB.open('/data/sensors.db');
   ```

5. **浏览器端（未来）**
   ```typescript
   // IndexedDB 后端
   const clientDB = await SynapseDB.open('indexeddb://my-graph');
   ```

### 核心价值主张

#### For Neo4j/TigerGraph 用户

> "当你的数据 < 1GB，为什么要启动一个服务器？"

#### For SQLite 用户

> "如果你需要图查询，这是最简单的升级路径"

#### For TypeScript 开发者

> "终于有了原生的、类型安全的图数据库"

#### For 学习者

> "从零到图查询，只需要 30 分钟"

### 定位声明

> **SynapseDB 不是要成为"更好的 Neo4j"**
>
> 而是要成为：
>
> - SQLite 在图数据库领域的对应物
> - TypeScript 生态的原生图存储方案
> - 嵌入式应用的首选图数据库
> - 学习图数据库的入门工具

**我们的口号：**

> "Not another Neo4j, but the SQLite of graph databases"

---

## 技术现状评估

### 已实现能力 ✅

#### 存储层

- **三元组存储**：Subject-Predicate-Object 模型
- **六维索引**：SPO, SOP, POS, PSO, OSP, OPS 全排列
- **分页机制**：支持大数据集的按需加载
- **压缩支持**：Brotli 压缩，减少 60% 存储空间
- **属性系统**：节点属性 + 边属性的 KV 存储

#### 查询层

```typescript
// 当前链式查询 API
db.find({ subject: 'Alice' })
  .follow('KNOWS') // 正向遍历
  .followReverse('WORKS_AT') // 反向遍历
  .where((f) => f.confidence > 0.8)
  .limit(10)
  .all();
```

#### 事务与并发

- **WAL v2**：Write-Ahead Logging 崩溃恢复
- **MVCC**：多版本并发控制，读写不阻塞
- **快照隔离**：epoch-based 一致性读
- **批次事务**：支持 txId 幂等性

#### 运维能力

- **自动压缩**：热点数据驱动的增量压缩
- **垃圾回收**：清理孤立页面
- **数据修复**：CRC 校验与自动修复
- **CLI 工具**：完整的运维命令集

### 能力差距分析

#### 与标准图数据库的差距

| 特性         | SynapseDB 现状          | 标准图数据库      | 差距评估 | 实现难度 |
| ------------ | ----------------------- | ----------------- | -------- | -------- |
| **基础遍历** | ✅ follow/followReverse | ✅ 模式匹配       | 语法不同 | ⭐⭐     |
| **属性过滤** | ✅ whereNodeProperty    | ✅ WHERE 子句     | 功能相似 | ⭐       |
| **节点标签** | ⚠️ 可用谓语模拟         | ✅ 原生标签系统   | 需要扩展 | ⭐⭐     |
| **变长路径** | ❌ 仅单步遍历           | ✅ [*1..n]        | 核心差距 | ⭐⭐⭐   |
| **最短路径** | ❌ 无                   | ✅ shortestPath() | 算法缺失 | ⭐⭐⭐   |
| **聚合函数** | ❌ 无                   | ✅ COUNT/SUM/AVG  | 框架缺失 | ⭐⭐⭐   |
| **分组操作** | ❌ 无                   | ✅ GROUP BY       | 需要实现 | ⭐⭐⭐   |
| **模式匹配** | ❌ 仅链式               | ✅ 复杂模式       | 核心差距 | ⭐⭐⭐⭐ |
| **子查询**   | ❌ 无                   | ✅ 嵌套查询       | 复杂特性 | ⭐⭐⭐⭐ |
| **事务支持** | ✅ WAL/MVCC             | ✅ ACID           | 已实现   | ✅       |
| **并发控制** | ✅ 读写分离             | ✅ 多版本         | 已实现   | ✅       |

#### 待实现能力列表

##### 查询增强

- ❌ 模式匹配：`(a)-[:KNOWS]->(b)`
- ❌ 变长路径：`[*1..5]`
- ❌ 最短路径算法
- ❌ 聚合函数：COUNT, SUM, AVG
- ❌ 分组：GROUP BY
- ❌ 联合查询：UNION
- ❌ 子查询

##### 标准兼容

- ❌ Cypher 解析器
- ❌ Gremlin 适配器
- ❌ GraphQL 接口

##### 高级特性

- ❌ 全文搜索
- ❌ 地理空间索引
- ❌ 图算法库（PageRank, 社区发现等）
- ❌ 分布式支持

---

## 发展阶段规划

### Phase 0: 基础巩固（v1.0.x）

**时间**：2025 Q1（已完成）
**目标**：稳定当前核心功能

✅ 已完成项：

- 核心存储引擎稳定
- 基础查询 API
- 事务与并发控制
- 自动运维工具

### Phase 1: 图查询基础（v1.1.0）

**时间**：2025 Q1-Q2（8周）
**目标**：引入图数据库核心概念

#### 1.1.0-alpha（第1-2周）

- [ ] 节点标签系统（Labels）
- [ ] 模式匹配 API 设计
- [ ] 基础路径查询

#### 1.1.0-beta（第3-4周）

- [ ] 变长路径实现 `[*min..max]`
- [ ] 双向遍历优化
- [ ] 路径返回格式

#### 1.1.0-rc（第5-6周）

- [ ] 聚合函数框架
- [ ] COUNT 实现
- [ ] GROUP BY 基础

#### 1.1.0-stable（第7-8周）

- [ ] 性能优化
- [ ] 完整测试覆盖
- [ ] 文档更新

### Phase 2: 查询语言支持（v1.2.0）

**时间**：2025 Q2-Q3（12周）
**目标**：实现 Cypher 子集

#### 1.2.0-alpha（第1-4周）

- [ ] Cypher 词法分析器
- [ ] 语法解析器（PEG.js/ANTLR）
- [ ] AST 设计

#### 1.2.0-beta（第5-8周）

- [ ] 查询计划器
- [ ] 优化器框架
- [ ] 执行引擎

#### 1.2.0-stable（第9-12周）

- [ ] 更多聚合函数（SUM, AVG, MIN, MAX）
- [ ] 排序（ORDER BY）
- [ ] 分页（SKIP, LIMIT）

### Phase 3: 高级查询特性（v1.3.0）

**时间**：2025 Q3-Q4（12周）
**目标**：完整查询能力

- [ ] 子查询支持
- [ ] WITH 子句
- [ ] UNION/UNION ALL
- [ ] OPTIONAL MATCH
- [ ] 存在性谓词（EXISTS）

### Phase 4: 图算法库（v1.4.0）

**时间**：2025 Q4（8周）
**目标**：内置常用图算法

- [ ] 路径算法（最短路径、所有路径）
- [ ] 中心性算法（PageRank、Betweenness）
- [ ] 社区发现（Louvain、Label Propagation）
- [ ] 相似度算法（Jaccard、Cosine）

### Phase 5: 生态系统（v2.0.0）

**时间**：2026 Q1（16周）
**目标**：完整的图数据库生态

- [ ] GraphQL 适配器
- [ ] REST API 服务器
- [ ] Web 可视化界面
- [ ] VS Code 扩展
- [ ] 数据导入导出工具
- [ ] 与 Neo4j 的兼容层

---

## 详细实施方案

### v1.1.0 图查询基础

#### 节点标签系统

```typescript
// 新增标签索引结构
interface LabelIndex {
  // 标签到节点ID的映射
  labelToNodes: Map<string, Set<number>>;
  // 节点ID到标签集的映射
  nodeToLabels: Map<number, Set<string>>;
}

// API 设计
interface LabeledNode {
  id: number;
  value: string;
  labels: string[];
  properties?: Record<string, unknown>;
}

// 使用示例
db.addNode('alice', {
  labels: ['Person', 'Developer'],
  properties: { age: 30, city: 'Beijing' },
});
```

#### 模式匹配 API

```typescript
// 模式构建器
class PatternBuilder {
  // 节点匹配
  node(alias?: string, conditions?: NodePattern): this;
  // 边匹配
  edge(direction: '->' | '<-' | '-', type?: string, alias?: string): this;
  // 路径变量
  path(alias: string): this;
  // 执行查询
  execute(): PatternResult;
}

// 使用示例
const result = db
  .pattern()
  .node('a', { labels: ['Person'], props: { name: 'Alice' } })
  .edge('->', 'KNOWS', 'k')
  .node('b', { labels: ['Person'] })
  .edge('->', 'WORKS_AT', 'w')
  .node('c', { labels: ['Company'] })
  .where('b.age > 25 AND k.since > 2020')
  .return(['a.name', 'b.name', 'c.name', 'k.since'])
  .execute();
```

#### 变长路径实现

```typescript
// 路径匹配配置
interface PathConfig {
  minLength?: number; // 最小跳数
  maxLength?: number; // 最大跳数
  predicates?: string[]; // 允许的边类型
  uniqueness?: 'NODE' | 'EDGE' | 'NONE'; // 唯一性约束
}

// BFS 实现
class PathFinder {
  findPaths(from: number, to: number | undefined, config: PathConfig): Path[] {
    const queue: QueueItem[] = [{ node: from, path: [], depth: 0 }];
    const results: Path[] = [];
    const visited = new Set<string>();

    while (queue.length > 0) {
      const current = queue.shift()!;

      if (current.depth >= (config.minLength ?? 1)) {
        if (!to || current.node === to) {
          results.push(current.path);
        }
      }

      if (current.depth < (config.maxLength ?? 5)) {
        // 扩展邻居
        const neighbors = this.getNeighbors(current.node, config.predicates);
        for (const [edge, neighbor] of neighbors) {
          const key = this.getVisitKey(neighbor, edge, config.uniqueness);
          if (!visited.has(key)) {
            visited.add(key);
            queue.push({
              node: neighbor,
              path: [...current.path, edge],
              depth: current.depth + 1,
            });
          }
        }
      }
    }

    return results;
  }
}
```

#### 聚合框架设计

```typescript
// 聚合管道
interface AggregationStage {
  type: 'GROUP' | 'COUNT' | 'SUM' | 'AVG' | 'MIN' | 'MAX';
  field?: string;
  alias: string;
}

class AggregationPipeline {
  private stages: AggregationStage[] = [];
  private data: FactRecord[] = [];

  groupBy(fields: string[]): this {
    this.stages.push({ type: 'GROUP', field: fields.join(','), alias: '_group' });
    return this;
  }

  count(alias: string = 'count'): this {
    this.stages.push({ type: 'COUNT', alias });
    return this;
  }

  sum(field: string, alias: string): this {
    this.stages.push({ type: 'SUM', field, alias });
    return this;
  }

  execute(): AggregateResult[] {
    let result = this.data;

    for (const stage of this.stages) {
      result = this.executeStage(result, stage);
    }

    return result;
  }

  private executeStage(data: any[], stage: AggregationStage): any[] {
    switch (stage.type) {
      case 'GROUP':
        return this.groupByField(data, stage.field!);
      case 'COUNT':
        return this.addCount(data, stage.alias);
      // ... 其他聚合操作
    }
  }
}
```

### v1.2.0 Cypher 解析器

#### 词法分析器设计

```typescript
// Token 定义
enum TokenType {
  // 关键字
  MATCH = 'MATCH',
  WHERE = 'WHERE',
  RETURN = 'RETURN',
  CREATE = 'CREATE',
  DELETE = 'DELETE',
  WITH = 'WITH',

  // 运算符
  ARROW_RIGHT = '->',
  ARROW_LEFT = '<-',
  DASH = '-',

  // 标识符
  IDENTIFIER = 'IDENTIFIER',
  LABEL = 'LABEL',

  // 字面量
  STRING = 'STRING',
  NUMBER = 'NUMBER',

  // 分隔符
  LPAREN = '(',
  RPAREN = ')',
  LBRACKET = '[',
  RBRACKET = ']',
  LBRACE = '{',
  RBRACE = '}',
}

class Lexer {
  private input: string;
  private position: number = 0;

  constructor(input: string) {
    this.input = input;
  }

  nextToken(): Token {
    this.skipWhitespace();

    // 识别关键字
    if (this.matchKeyword('MATCH')) return { type: TokenType.MATCH, value: 'MATCH' };
    if (this.matchKeyword('WHERE')) return { type: TokenType.WHERE, value: 'WHERE' };

    // 识别运算符
    if (this.match('->')) return { type: TokenType.ARROW_RIGHT, value: '->' };
    if (this.match('<-')) return { type: TokenType.ARROW_LEFT, value: '<-' };

    // 识别标识符
    if (this.isLetter()) return this.readIdentifier();

    // 识别数字
    if (this.isDigit()) return this.readNumber();

    // 识别字符串
    if (this.current() === "'" || this.current() === '"') return this.readString();

    // ... 更多 token 识别
  }
}
```

#### 语法解析器（使用 PEG.js）

```pegjs
// cypher.pegjs
Query
  = _ clauses:Clause+ _ { return { type: 'Query', clauses } }

Clause
  = MatchClause
  / WhereClause
  / ReturnClause
  / WithClause

MatchClause
  = "MATCH" _ pattern:Pattern _ {
      return { type: 'MATCH', pattern }
    }

Pattern
  = path:Path { return path }

Path
  = node:Node relationships:RelationshipPattern* {
      return { type: 'Path', start: node, relationships }
    }

Node
  = "(" _ variable:Identifier? _ labels:Labels? _ props:Properties? _ ")" {
      return { type: 'Node', variable, labels, props }
    }

RelationshipPattern
  = "-" relationship:Relationship "->" _ node:Node {
      return { type: 'Outgoing', relationship, node }
    }
  / "<-" relationship:Relationship "-" _ node:Node {
      return { type: 'Incoming', relationship, node }
    }

Relationship
  = "[" _ variable:Identifier? _ type:RelType? _ props:Properties? _ "]" {
      return { variable, type, props }
    }

Labels
  = ":" label:Identifier labels:(":" Identifier)* {
      return [label, ...labels.map(l => l[1])]
    }

Properties
  = "{" _ props:PropertyList? _ "}" { return props || {} }

PropertyList
  = head:Property tail:(_ "," _ Property)* {
      const result = { [head.key]: head.value };
      tail.forEach(t => result[t[3].key] = t[3].value);
      return result;
    }

Property
  = key:Identifier _ ":" _ value:Literal {
      return { key, value }
    }
```

#### 查询计划器

```typescript
// 查询计划节点
interface PlanNode {
  type: 'Scan' | 'Filter' | 'Expand' | 'Join' | 'Aggregate' | 'Project';
  cost: number;
  cardinality: number;
  children: PlanNode[];
}

class QueryPlanner {
  // 从 AST 生成逻辑计划
  generateLogicalPlan(ast: AST): LogicalPlan {
    const builder = new LogicalPlanBuilder();

    for (const clause of ast.clauses) {
      switch (clause.type) {
        case 'MATCH':
          builder.addMatch(clause.pattern);
          break;
        case 'WHERE':
          builder.addFilter(clause.predicate);
          break;
        case 'RETURN':
          builder.addProjection(clause.items);
          break;
      }
    }

    return builder.build();
  }

  // 优化逻辑计划为物理计划
  optimizePlan(logical: LogicalPlan): PhysicalPlan {
    // 1. 谓词下推
    logical = this.pushDownPredicates(logical);

    // 2. 选择最优索引
    logical = this.selectIndexes(logical);

    // 3. 连接顺序优化
    logical = this.optimizeJoinOrder(logical);

    // 4. 生成物理操作符
    return this.generatePhysicalOperators(logical);
  }

  // 基于统计信息估算成本
  estimateCost(node: PlanNode): number {
    switch (node.type) {
      case 'Scan':
        return this.estimateScanCost(node);
      case 'Filter':
        return this.estimateFilterCost(node);
      case 'Expand':
        return this.estimateExpandCost(node);
      // ...
    }
  }
}
```

#### 执行引擎

```typescript
// 执行器接口
interface Executor {
  execute(plan: PhysicalPlan, context: ExecutionContext): AsyncIterator<Record>;
}

// 火山模型执行器
class VolcanoExecutor implements Executor {
  async *execute(plan: PhysicalPlan, context: ExecutionContext): AsyncIterator<Record> {
    const operator = this.createOperator(plan.root);

    await operator.open();
    try {
      while (true) {
        const tuple = await operator.next();
        if (!tuple) break;
        yield tuple;
      }
    } finally {
      await operator.close();
    }
  }

  private createOperator(node: PlanNode): Operator {
    switch (node.type) {
      case 'TableScan':
        return new TableScanOperator(node.table, node.filters);
      case 'IndexScan':
        return new IndexScanOperator(node.index, node.range);
      case 'NestedLoopJoin':
        return new NestedLoopJoinOperator(
          this.createOperator(node.left),
          this.createOperator(node.right),
          node.condition,
        );
      case 'HashJoin':
        return new HashJoinOperator(
          this.createOperator(node.left),
          this.createOperator(node.right),
          node.keys,
        );
      // ...
    }
  }
}
```

### v1.3.0 高级查询特性

#### 子查询支持

```typescript
// 子查询类型
type SubqueryType = 'EXISTS' | 'SCALAR' | 'IN' | 'CORRELATED';

interface Subquery {
  type: SubqueryType;
  query: Query;
  correlation?: string[]; // 关联变量
}

// 子查询执行策略
class SubqueryExecutor {
  execute(subquery: Subquery, parentContext: Context): any {
    switch (subquery.type) {
      case 'EXISTS':
        return this.executeExists(subquery, parentContext);
      case 'SCALAR':
        return this.executeScalar(subquery, parentContext);
      case 'IN':
        return this.executeIn(subquery, parentContext);
      case 'CORRELATED':
        return this.executeCorrelated(subquery, parentContext);
    }
  }

  private executeCorrelated(subquery: Subquery, parent: Context): any {
    // 对父查询的每一行执行子查询
    const results = [];
    for (const row of parent.rows) {
      // 绑定关联变量
      const childContext = this.bindCorrelation(subquery, row);
      const result = this.executeQuery(subquery.query, childContext);
      results.push(result);
    }
    return results;
  }
}
```

### v1.4.0 图算法实现

#### PageRank 算法

```typescript
class PageRankAlgorithm {
  private damping = 0.85;
  private tolerance = 0.0001;
  private maxIterations = 100;

  compute(graph: Graph): Map<number, number> {
    const nodeCount = graph.nodeCount();
    const scores = new Map<number, number>();

    // 初始化分数
    for (const node of graph.nodes()) {
      scores.set(node, 1.0 / nodeCount);
    }

    // 迭代计算
    for (let iter = 0; iter < this.maxIterations; iter++) {
      const newScores = new Map<number, number>();
      let diff = 0;

      for (const node of graph.nodes()) {
        let score = (1 - this.damping) / nodeCount;

        // 累加入边贡献
        for (const inEdge of graph.inEdges(node)) {
          const sourceScore = scores.get(inEdge.source)!;
          const outDegree = graph.outDegree(inEdge.source);
          score += this.damping * (sourceScore / outDegree);
        }

        newScores.set(node, score);
        diff += Math.abs(score - scores.get(node)!);
      }

      scores = newScores;

      // 收敛检查
      if (diff < this.tolerance) break;
    }

    return scores;
  }
}
```

#### 最短路径算法

```typescript
// Dijkstra 算法实现
class ShortestPath {
  dijkstra(
    graph: Graph,
    source: number,
    target?: number,
  ): Map<number, { distance: number; path: number[] }> {
    const distances = new Map<number, number>();
    const previous = new Map<number, number>();
    const pq = new PriorityQueue<number>((a, b) => distances.get(a)! - distances.get(b)!);

    // 初始化
    for (const node of graph.nodes()) {
      distances.set(node, node === source ? 0 : Infinity);
      pq.enqueue(node);
    }

    while (!pq.isEmpty()) {
      const current = pq.dequeue()!;

      if (target && current === target) break;

      for (const edge of graph.outEdges(current)) {
        const alt = distances.get(current)! + edge.weight;
        if (alt < distances.get(edge.target)!) {
          distances.set(edge.target, alt);
          previous.set(edge.target, current);
          pq.updatePriority(edge.target);
        }
      }
    }

    // 构建结果
    const result = new Map();
    for (const [node, dist] of distances) {
      if (dist !== Infinity) {
        result.set(node, {
          distance: dist,
          path: this.reconstructPath(previous, source, node),
        });
      }
    }

    return result;
  }
}
```

---

## 技术架构演进

### 当前架构（v1.0）

```
┌─────────────────────────────────────────┐
│           应用层 (Application)          │
├─────────────────────────────────────────┤
│         查询层 (Query Builder)          │
├─────────────────────────────────────────┤
│          存储层 (Storage)               │
│  ┌──────────┬──────────┬──────────┐   │
│  │ TripleStore │ PropertyStore │ WAL │   │
│  └──────────┴──────────┴──────────┘   │
├─────────────────────────────────────────┤
│          索引层 (Indexes)               │
│  ┌──────────┬──────────┬──────────┐   │
│  │   SPO    │   POS    │   OSP    │   │
│  └──────────┴──────────┴──────────┘   │
└─────────────────────────────────────────┘
```

### 目标架构（v2.0）

```
┌─────────────────────────────────────────┐
│           应用层 (Applications)         │
│   ┌──────┬──────┬──────┬──────┐       │
│   │ REST │ GraphQL │ gRPC │ SDK │      │
│   └──────┴──────┴──────┴──────┘       │
├─────────────────────────────────────────┤
│        查询语言层 (Query Languages)     │
│   ┌──────────┬──────────┬──────────┐  │
│   │  Cypher  │  Gremlin │  Native  │  │
│   └──────────┴──────────┴──────────┘  │
├─────────────────────────────────────────┤
│         查询处理层 (Query Processing)   │
│   ┌─────────┬─────────┬──────────┐    │
│   │ Parser  │ Planner │ Optimizer │   │
│   └─────────┴─────────┴──────────┘    │
├─────────────────────────────────────────┤
│         执行层 (Execution)              │
│   ┌─────────┬─────────┬──────────┐    │
│   │ Runtime │ Cache   │ Statistics│   │
│   └─────────┴─────────┴──────────┘    │
├─────────────────────────────────────────┤
│         图模型层 (Graph Model)          │
│   ┌──────────┬──────────┬──────────┐  │
│   │  Nodes   │  Edges   │  Paths   │  │
│   └──────────┴──────────┴──────────┘  │
├─────────────────────────────────────────┤
│         存储引擎 (Storage Engine)       │
│   ┌──────────┬──────────┬──────────┐  │
│   │ TripleStore │ LabelIndex │ PropIdx│ │
│   └──────────┴──────────┴──────────┘  │
├─────────────────────────────────────────┤
│         持久化层 (Persistence)          │
│   ┌──────────┬──────────┬──────────┐  │
│   │   WAL    │   Pages  │  Compact │  │
│   └──────────┴──────────┴──────────┘  │
└─────────────────────────────────────────┘
```

---

## API 设计规范

### 核心 API 演进

#### v1.0 (当前)

```typescript
// 基础三元组 API
db.addFact({ subject: 'A', predicate: 'R', object: 'B' });
db.find({ subject: 'A' }).follow('R').all();
```

#### v1.1 (模式匹配)

```typescript
// 模式匹配 API
db.match()
  .pattern('(a:Person)-[:KNOWS]->(b:Person)')
  .where('a.name = "Alice"')
  .return(['b.name', 'b.age']);
```

#### v1.2 (Cypher)

```typescript
// Cypher 查询
await db.cypher(`
  MATCH (a:Person {name: 'Alice'})-[:KNOWS*1..3]->(b:Person)
  WHERE b.age > 25
  RETURN b.name, COUNT(*) as count
  ORDER BY count DESC
  LIMIT 10
`);
```

#### v2.0 (完整生态)

```typescript
// GraphQL 接口
const schema = buildSchema(`
  type Person {
    name: String!
    age: Int
    knows: [Person]
  }

  type Query {
    person(name: String!): Person
    shortestPath(from: String!, to: String!): [Person]
  }
`);

// REST API
app.get('/api/nodes/:label', async (req, res) => {
  const nodes = await db.nodes(req.params.label).all();
  res.json(nodes);
});

// 流式处理
const stream = db.stream(
  `
  MATCH (n:Person)
  RETURN n
`,
  { batchSize: 1000 },
);

for await (const batch of stream) {
  await processBatch(batch);
}
```

### 类型系统增强

```typescript
// 强类型支持
interface PersonNode {
  name: string;
  age: number;
  email?: string;
}

interface KnowsEdge {
  since: Date;
  strength: number;
}

// 类型安全的查询
const result = await db
  .typed<PersonNode>()
  .match('(p:Person)')
  .where((p) => p.age > 25)
  .return((p) => ({
    name: p.name,
    email: p.email,
  }));
```

---

## 性能基准目标

### 查询性能目标

| 操作类型 | 数据规模   | v1.0 (当前) | v1.1 目标 | v2.0 目标 |
| -------- | ---------- | ----------- | --------- | --------- |
| 单跳查询 | 100K nodes | < 10ms      | < 10ms    | < 5ms     |
| 2跳查询  | 100K nodes | < 50ms      | < 30ms    | < 20ms    |
| 3跳查询  | 100K nodes | < 200ms     | < 100ms   | < 50ms    |
| 模式匹配 | 100K nodes | N/A         | < 100ms   | < 50ms    |
| 聚合查询 | 100K nodes | N/A         | < 200ms   | < 100ms   |
| 最短路径 | 100K nodes | N/A         | < 500ms   | < 200ms   |

### 存储效率目标

| 指标     | v1.0 (当前) | v1.1 目标 | v2.0 目标 |
| -------- | ----------- | --------- | --------- |
| 压缩率   | 60%         | 65%       | 70%       |
| 索引大小 | 2x data     | 1.8x data | 1.5x data |
| 写入吞吐 | 10K/s       | 15K/s     | 20K/s     |
| 并发读者 | 100         | 500       | 1000      |

### 内存使用目标

| 场景       | v1.0 (当前) | v1.1 目标 | v2.0 目标 |
| ---------- | ----------- | --------- | --------- |
| 空载内存   | < 10MB      | < 10MB    | < 15MB    |
| 100K nodes | < 100MB     | < 80MB    | < 60MB    |
| 1M nodes   | < 1GB       | < 800MB   | < 600MB   |
| 查询缓存   | N/A         | 可配置    | 自适应    |

---

## 风险评估与缓解

### 技术风险

#### 风险1：查询语言解析器复杂度

- **影响**：开发周期延长，bug 增多
- **缓解**：
  - 使用成熟的解析器生成器（PEG.js/ANTLR）
  - 分阶段实现，先支持子集
  - 大量的测试用例覆盖

#### 风险2：性能退化

- **影响**：新特性影响现有性能
- **缓解**：
  - 建立性能基准测试套件
  - CI/CD 集成性能测试
  - 特性开关，可降级

#### 风险3：向后兼容性

- **影响**：破坏现有用户代码
- **缓解**：
  - 严格的语义版本控制
  - 废弃 API 的渐进式迁移
  - 提供迁移工具和指南

### 资源风险

#### 风险4：开发资源不足

- **影响**：延期交付
- **缓解**：
  - 优先级排序，核心特性优先
  - 寻求社区贡献
  - 考虑商业支持模式

### 市场风险

#### 风险5：竞争产品

- **影响**：用户流失
- **缓解**：
  - 差异化定位（嵌入式、轻量级）
  - 快速迭代，响应用户需求
  - 建立技术护城河

---

## 社区建设计划

### 开发者生态

#### 文档体系

- **入门教程**：5分钟快速上手
- **进阶指南**：最佳实践、性能调优
- **API 参考**：完整的 API 文档
- **示例项目**：真实场景的完整示例

#### 开发工具

- **VS Code 扩展**：语法高亮、自动补全
- **在线 Playground**：浏览器中体验
- **迁移工具**：从 Neo4j/TigerGraph 迁移
- **基准测试框架**：性能对比工具

### 社区运营

#### 沟通渠道

- **GitHub Discussions**：技术讨论
- **Discord/Slack**：实时交流
- **技术博客**：深度技术文章
- **视频教程**：B站/YouTube

#### 贡献机制

- **贡献指南**：详细的贡献流程
- **Good First Issue**：新手友好任务
- **导师制度**：老手带新人
- **贡献者认证**：贡献者荣誉体系

### 商业模式探索

#### 开源版本

- MIT 协议
- 完整功能
- 社区支持

#### 商业版本（可选）

- 企业级功能（集群、监控）
- SLA 支持
- 定制开发
- 培训服务

---

## 长期发展展望

### 3年愿景（2025-2027）

#### 技术目标

- ✅ 成为 TypeScript 生态最好的嵌入式图数据库
- ✅ 支持主流图查询语言（Cypher、Gremlin）
- ✅ 内置丰富的图算法库
- ✅ 完善的可视化和开发工具

#### 生态目标

- 📊 GitHub Star 10K+
- 👥 活跃贡献者 50+
- 📦 npm 周下载 10K+
- 🏢 生产环境案例 100+

#### 应用场景

- **知识图谱**：企业知识管理
- **代码分析**：代码依赖分析、架构可视化
- **推荐系统**：社交推荐、内容推荐
- **网络安全**：威胁情报、攻击路径分析
- **生物信息**：蛋白质相互作用网络

### 5年展望（2025-2029）

#### 分布式版本

- 支持分片和复制
- 跨节点查询优化
- 一致性协议（Raft/Paxos）

#### AI 集成

- 向量搜索支持
- 图神经网络集成
- 自动查询优化

#### 云原生

- Kubernetes Operator
- Serverless 部署
- 多云支持

---

## 未来差异化技术路线

### 独特技术方向（2026-2028）

#### 1. WebAssembly 版本

```typescript
// 浏览器原生运行
import { SynapseDB } from '@synapsedb/wasm';

const db = await SynapseDB.open({
  backend: 'indexeddb',
  cache: 'memory',
});

// 完整的图查询能力在浏览器中
const result = await db.cypher(`
  MATCH (n:Person)-[:KNOWS]->(m)
  RETURN n, m
`);
```

**技术优势**：

- 跨平台一致性（浏览器、Node.js、Deno、Bun）
- 接近原生性能
- 零安装，CDN 直接引用
- 支持离线 PWA 应用

#### 2. AI 原生集成

```typescript
// 向量 + 图的混合查询
interface AIEnhancedDB {
  // 向量相似度搜索
  vectorSearch(embedding: number[], k: number): NodeResult[];

  // 图结构 + 向量的混合查询
  hybridQuery(): HybridQueryBuilder;

  // 自动图构建
  extractGraph(text: string, model: 'gpt-4' | 'claude'): Graph;
}

// 使用示例
const similar = await db
  .vectorSearch(queryEmbedding, 10)
  .follow('RELATED_TO')
  .filter((node) => node.score > 0.8)
  .all();

// RAG 增强
const context = await db
  .findSimilar(question)
  .expandContext(2) // 2跳扩展
  .generateAnswer(llm);
```

**应用场景**：

- RAG（检索增强生成）系统
- 智能问答
- 推荐系统
- 知识发现

#### 3. 代码理解特化

```typescript
// 专门的代码分析 API
class CodeGraph extends SynapseDB {
  // 自动解析代码结构
  async analyzeCode(path: string): Promise<CodeAnalysis> {
    const ast = await this.parseAST(path);
    const graph = await this.buildDependencyGraph(ast);
    return {
      dependencies: graph,
      metrics: this.calculateMetrics(graph),
      issues: this.detectIssues(graph),
    };
  }

  // 架构分析
  detectArchitecturalPatterns(): Pattern[] {
    return this.cypher(`
      MATCH (m:Module)-[:DEPENDS_ON]->(n:Module)
      WHERE m.layer = 'presentation' AND n.layer = 'data'
      RETURN m, n as violation
    `);
  }

  // 影响分析
  impactAnalysis(file: string): ImpactResult {
    return this.cypher(
      `
      MATCH (f:File {path: $file})-[:DEPENDS_ON*1..5]->(affected)
      RETURN affected, min(length(path)) as distance
      ORDER BY distance
    `,
      { file },
    );
  }
}
```

**特色功能**：

- AST 级别的代码分析
- 自动架构图生成
- 循环依赖检测
- 代码质量度量
- 重构建议

#### 4. 实时协作图数据库

```typescript
// P2P 同步与协作
interface CollaborativeDB {
  // CRDT-based 同步
  sync(options: {
    peers: string[];
    strategy: 'crdt' | 'ot' | 'last-write-wins';
    conflictResolver?: (a: any, b: any) => any;
  }): void;

  // 实时订阅
  subscribe(pattern: string, callback: (change: Change) => void): void;

  // 分支与合并
  branch(name: string): BranchDB;
  merge(branch: string, strategy: MergeStrategy): void;
}

// 使用场景：多人协作知识库
const db = await SynapseDB.open('collab://project-kb');

db.sync({
  peers: ['wss://peer1.example.com', 'wss://peer2.example.com'],
  strategy: 'crdt',
});

db.subscribe('(n:Task {status: "new"})', (change) => {
  console.log('New task created:', change);
  notifyTeam(change);
});
```

**应用场景**：

- 团队知识管理
- 分布式白板
- 协作式思维导图
- 多人游戏状态同步

#### 5. 时序图数据库

```typescript
// 时间维度的图查询
interface TemporalGraph {
  // 时间点查询
  at(timestamp: Date): GraphSnapshot;

  // 时间范围查询
  between(start: Date, end: Date): TemporalQueryBuilder;

  // 演化分析
  evolution(pattern: string): EvolutionResult[];
}

// 使用示例
// 查询特定时间点的好友关系
const friends2023 = await db
  .at(new Date('2023-01-01'))
  .match('(p:Person {name: "Alice"})-[:KNOWS]->(friend)')
  .return('friend');

// 分析关系演化
const evolution = await db
  .between(startDate, endDate)
  .track('(p:Person)-[r:KNOWS]->()')
  .groupBy('month')
  .aggregate('count');
```

**应用场景**：

- 社交网络演化分析
- 代码库历史分析
- 金融交易网络
- 供应链追踪

#### 6. 联邦图查询

```typescript
// 跨多个 SynapseDB 实例查询
interface FederatedDB {
  // 注册远程数据源
  addRemote(name: string, url: string): void;

  // 联邦查询
  federated(): FederatedQueryBuilder;
}

// 使用示例
const fed = new FederatedDB();
fed.addRemote('users', 'synapsedb://server1/users');
fed.addRemote('products', 'synapsedb://server2/products');
fed.addRemote('orders', 'synapsedb://server3/orders');

// 跨库查询
const result = await fed.federated().cypher(`
  MATCH (u:User)@users-[:PURCHASED]->(o:Order)@orders
  MATCH (o)-[:CONTAINS]->(p:Product)@products
  RETURN u.name, collect(p.name) as products
`);
```

**应用场景**：

- 微服务数据聚合
- 多租户系统
- 数据湖查询
- 跨部门数据分析

### 技术护城河构建

#### 核心竞争力

1. **极致的轻量化**：始终保持 < 1MB 运行时
2. **TypeScript 原生**：最好的类型支持和开发体验
3. **嵌入式优先**：不需要服务器的图数据库
4. **渐进式增强**：从简单 API 到完整 Cypher

#### 生态系统建设

1. **插件系统**：支持自定义函数、算法、存储后端
2. **适配器生态**：React、Vue、Svelte 等框架集成
3. **工具链**：迁移工具、可视化工具、性能分析工具
4. **教育资源**：互动教程、视频课程、认证体系

#### 社区驱动创新

1. **RFC 流程**：重大特性通过 RFC 讨论
2. **插件市场**：社区贡献的扩展
3. **基准测试**：公开、透明的性能对比
4. **案例展示**：真实项目的最佳实践

---

## 实施时间表

### 2025 Q1（1-3月）

- [x] v1.0.0 发布（基础稳定版）
- [ ] v1.1.0-alpha（模式匹配）
- [ ] 性能基准建立
- [ ] 文档体系完善

### 2025 Q2（4-6月）

- [ ] v1.1.0 正式版
- [ ] v1.2.0-alpha（Cypher 解析器）
- [ ] VS Code 扩展
- [ ] 在线 Playground

### 2025 Q3（7-9月）

- [ ] v1.2.0 正式版
- [ ] v1.3.0-alpha（高级查询）
- [ ] 性能优化专项
- [ ] 社区建设

### 2025 Q4（10-12月）

- [ ] v1.3.0 正式版
- [ ] v1.4.0（图算法库）
- [ ] 商业版本探索
- [ ] 年度总结与规划

### 2026 展望

- [ ] v2.0.0（完整生态）
- [ ] 分布式版本原型
- [ ] AI 功能集成
- [ ] 国际化推广

---

## 关键成功因素

### 技术卓越

- 🎯 性能始终是第一优先级
- 🎯 代码质量和测试覆盖率
- 🎯 文档和示例的完整性

### 社区驱动

- 🤝 倾听用户声音
- 🤝 快速响应和修复
- 🤝 透明的开发过程

### 差异化定位

- 💡 坚持"嵌入式"和"轻量级"
- 💡 TypeScript 原生体验
- 💡 易用性优于功能完整性

### 持续创新

- 🚀 跟踪前沿技术
- 🚀 探索新的应用场景
- 🚀 保持技术领先性

---

## 联系方式

- **GitHub**: https://github.com/[org]/SynapseDB
- **Email**: synapsedb@[domain].com
- **Discord**: https://discord.gg/synapsedb
- **Twitter**: @synapsedb

---

_本路线图为动态文档，将根据社区反馈和技术发展持续更新。_

_最后更新：2025-01-24_
