# 变更日志

## 未发布

### ✨ 新增能力
- **多查询语言执行器**：补齐 Cypher、GraphQL、Gremlin 方言解析与执行管线，标准语法与 QueryBuilder 共享存储/索引层。
- **全文检索引擎**：引入倒排索引、批量索引 API、评分器与查询 DSL，覆盖批处理/在线检索场景。
- **空间索引与查询**：提供 R-Tree、几何运算与范围/相交/最近邻查询能力，统一通过属性扩展写入。
- **图算法套件**：实现最短路径（Dijkstra/A*/双向 BFS）、中心性与社区发现算法，配合链式查询与属性过滤。
- **治理 CLI 扩展**：完善自动压实（增量/整序混合）、热度驱动策略、事务 ID 观测与页级修复命令。

### ⚠️ 弃用声明

#### 基准测试框架迁移

**背景**：内部基准测试框架 `src/benchmark/**` 拖累覆盖率统计（17.36%），且与外部脚本 `benchmarks/*.mjs` 功能重复。

**迁移计划**：
- ✅ **v1.1.x**（当前）：`src/benchmark/**` 标记为弃用，CLI 命令（`pnpm benchmark`）保持兼容但内部已切换到外部脚本
- 📅 **v2.0**：完全移除 `src/benchmark/**` 模块

**推荐操作**：
```bash
# 方式1：继续使用 CLI（内部已迁移到外部脚本）
pnpm benchmark run           # 运行完整测试
pnpm benchmark core          # 运行核心测试
pnpm benchmark search        # 运行全文搜索测试
pnpm benchmark graph         # 运行图算法测试
pnpm benchmark spatial       # 运行空间几何测试

# 方式2：直接运行外部脚本（推荐，获得最佳体验）
node benchmarks/run-all.mjs --suite=all --format=console,json
node benchmarks/quick.mjs                        # 快速测试
node benchmarks/comprehensive.mjs                # 综合测试
node benchmarks/insert_scan.mjs                  # 插入与扫描
node benchmarks/path_agg.mjs                     # 路径与聚合
```

**影响面**：
- CLI 命令接口不变，用户无感知
- 程序化引用 `import {} from '@/benchmark'` 不推荐（未作为公开 API）
- `regression` 和 `memory-leak` 命令暂时保留内部实现，将在后续版本迁移

### 📚 文档
- 更新根目录 `README.md`，补充架构、数据模型、运维与调优细节。
- 重写 `docs/教学文档` 教程 00~09 及附录、实战章节，统一"目标→步骤→验证→FAQ"结构。
- 全面更新 `docs/使用示例`，覆盖 CLI、查询、事务、图算法、全文/空间索引、迁移指南等场景。

## v1.1.0 - 基础巩固里程碑 🎉

**发布日期**：2025-01-24
**里程碑状态**：✅ 完成所有目标（149 个测试通过）

### 🚀 主要特性

#### 性能优化与稳定性修复

- **✅ 文件句柄泄漏修复**：修复 `src/storage/wal.ts` 中的文件句柄管理错误
- **✅ 内存泄漏彻底解决**：`persistentStore.ts` 增加全面的 `close()` 清理机制
- **✅ Manifest 写入性能优化**：减少 fsync 调用，实现批量更新机制
- **✅ WAL 嵌套 ABORT 语义**：验证并完善嵌套事务回滚逻辑

#### 算法与查询优化

- **✅ Dijkstra 算法优化**：引入 MinHeap 数据结构，复杂度从 O(n²) 优化到 O((V+E)logV)
- **✅ 双向 BFS 路径查询**：实现查询缓存与 Set-based 数据结构优化
- **✅ 流式聚合执行**：支持增量聚合计算，内存占用从 O(n) 降到 O(1)
- **✅ 流式查询迭代器**：完整的异步迭代器支持，支持 `for await...of` 语法

#### 图数据库核心功能

- **✅ 节点标签系统**：支持 Neo4j 风格的 `[:Person]` 标签查询和多标签 AND/OR 组合
- **✅ 变长路径查询**：完整的 `[*1..5]` 语法支持，包含最短路径算法
- **✅ 聚合函数框架**：实现 COUNT、SUM、AVG、GROUP BY 等完整聚合能力
- **✅ 属性索引优化**：支持属性过滤下推到存储层，显著提升查询性能

#### 工程化质量提升

- **✅ TypeScript 类型系统增强**：
  - 完整的泛型化 API 设计 (`TypedSynapseDB<TNode, TEdge>`)
  - 编译时类型安全与运行时兼容性并存
  - 预定义类型：`PersonNode`、`RelationshipEdge`、`EntityNode`、`KnowledgeEdge`
  - 类型安全的查询构建器与属性访问
- **✅ 性能基准测试套件**：
  - 建立完整的基准测试框架 (`benchmarks/framework.mjs`)
  - 综合性能测试套件 (`benchmarks/comprehensive.mjs`)
  - CI 快速测试集成 (`benchmarks/quick.mjs`)
- **✅ 测试覆盖率提升**：149 个测试用例，覆盖所有核心功能模块

### 📦 新增模块

- `src/utils/minHeap.ts` - MinHeap 数据结构实现
- `src/types/enhanced.ts` - 完整的 TypeScript 类型定义
- `src/typedSynapseDb.ts` - 类型安全 API 包装器
- `benchmarks/` - 性能基准测试框架
- `src/query/aggregation.ts` - 聚合函数核心实现

### 📈 性能提升对比

| 功能              | 优化前 | 优化后       | 提升比例   |
| ----------------- | ------ | ------------ | ---------- |
| 大数据集查询内存  | ~1GB   | <100MB       | 90% ↓      |
| Dijkstra 最短路径 | O(n²)  | O((V+E)logV) | 数量级提升 |
| 属性过滤查询      | ~500ms | <50ms        | 90% ↓      |
| 流式聚合内存      | O(n)   | O(1)         | 内存稳定   |

### 🔧 API 变更

#### 新增 API

```typescript
// TypeScript 类型安全 API
const db = await TypedSynapseDB.open<PersonNode, RelationshipEdge>('./db.synapsedb');

// 标签查询
db.findByLabel('Person', { mode: 'AND' });
db.findByLabel(['Person', 'Employee'], { mode: 'OR' });

// 属性查询优化
db.findByNodeProperty({ propertyName: 'age', range: { min: 25, max: 35 } });
db.findByEdgeProperty({ propertyName: 'weight', value: 0.8 });

// 聚合查询
db.aggregate().match({ predicate: 'KNOWS' }).groupBy(['subject']).count('friendCount').execute();

// 流式查询
for await (const record of db.find({})) {
  console.log(record);
}
```

#### 向后兼容性

- ✅ 所有现有 API 完全兼容
- ✅ 现有数据库文件格式兼容
- ✅ 配置参数向后兼容

### 🧪 测试状态

- **单元测试**：149 通过 / 1 跳过 / 0 失败 ✅
- **集成测试**：所有核心功能验证通过 ✅
- **性能测试**：所有性能目标达成 ✅
- **内存泄漏测试**：长时间运行无内存增长 ✅

### 🎯 下一步计划

v1.1.0 基础巩固完成后，系统现已准备进入 **v1.2.0 查询增强阶段**，将实现：

- 🔥 模式匹配查询：`(a)-[:KNOWS]->(b)` 语法
- 🔥 高级变长路径：完整的算法套件
- 🔥 联合查询与子查询：UNION、EXISTS、IN/NOT IN 支持

---

## v0.2.0

- P0：WAL v2 合流与清理、尾部安全截断测试、写锁/读者参数对齐、基础 CI 接入
- P1：读快照一致性 `withSnapshot(fn)`、QueryBuilder 链路固定 epoch、运维组合用例与文档补充
- P2（阶段一）：
  - 事务批次原型增强：`beginBatch({ txId?, sessionId? })`，WAL `BEGIN` 携带元信息
  - 重放幂等：相同 `txId` 的重复 COMMIT 跳过；属性与 abort 语义测试覆盖
  - 持久化去重（可选）：`enablePersistentTxDedupe` + `maxRememberTxIds` + `txids.json`
  - CLI：`db:txids`、`db:stats --txids[=N]`；README 与设计文档新增说明

> 重要：旧 WAL 可兼容重放；若出现历史不一致，建议先执行 `pnpm db:repair` 与 `pnpm db:check --strict`。
