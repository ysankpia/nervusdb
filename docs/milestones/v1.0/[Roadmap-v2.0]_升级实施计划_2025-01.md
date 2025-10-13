# NervusDB v1.0 升级实施计划 (TODOs)

> 基于 Roadmap v2.0 的详细工程实施任务清单
>
> **当前版本**: v1.0-beta (所有90个测试通过)
> **目标版本**: v1.0 正式版
> **最后更新**: 2025-01-24

## 📊 项目现状评估

### ✅ 已完成功能

- [x] 核心三元组存储引擎
- [x] WAL v2 崩溃恢复机制
- [x] 分页索引系统
- [x] 链式联想查询 (QueryBuilder)
- [x] 读快照一致性 (withSnapshot)
- [x] 增量压缩算法
- [x] 自动压缩策略
- [x] 垃圾回收机制
- [x] 热点数据追踪
- [x] 事务幂等性支持
- [x] 进程级写锁保护
- [x] 完整的 CLI 运维工具集
- [x] **属性倒排索引系统** ✨ **NEW**
- [x] **纯磁盘查询引擎** (queryFromDisk) ✨ **NEW**
- [x] **内存优化的快照隔离** ✨ **NEW**
- [x] **读者注册表与 Epoch 追踪** ✨ **NEW**
- [x] **增强的可观测性工具** (stats, readers) ✨ **NEW**

### ✅ 已解决的技术债务

- [x] FileHandle 垃圾回收警告 ✅ **已修复**
- [x] 内存使用优化（实现纯磁盘查询 queryFromDisk）✅ **已完成**
- [x] 查询性能瓶颈（属性倒排索引已实现）✅ **已完成**

---

## 🎯 里程碑 B: Beta 强化与效率调优 (P1 - 高优先级)

### 📝 B.0: 技术债务清理

**预估工时**: 2天
**优先级**: 🔴 紧急

#### B.0.1 修复 FileHandle 垃圾回收警告

- [ ] **问题定位**: 分析所有文件操作代码，找出未正确关闭的 FileHandle
  - 检查文件: `src/storage/persistentStore.ts`
  - 检查文件: `src/storage/pagedIndex.ts`
  - 检查文件: `src/storage/wal.ts`
  - 检查文件: `src/maintenance/compaction.ts`
- [ ] **实现修复**: 确保所有 FileHandle 使用 try-finally 模式
  ```typescript
  // 需要修改的模式
  const handle = await fs.open(path, 'r');
  try {
    // 操作
  } finally {
    await handle.close();
  }
  ```
- [ ] **测试验证**: 运行所有测试，确保警告消失
  - 运行: `pnpm test --silent 2>&1 | grep -i "file descriptor"`
  - 预期: 无输出

---

### 📝 B.1: 优化快照隔离的内存占用

**预估工时**: 5天
**优先级**: 🔴 高
**影响文件**:

- `src/storage/persistentStore.ts`
- `src/maintenance/gc.ts`
- `src/storage/readerRegistry.ts`
- `src/storage/pagedIndex.ts`

#### B.1.1 重构快照查询机制

- [ ] **分析现状**: 审查 `PersistentStore.query()` 的快照实现
  - 定位内存 TripleStore 依赖点
  - 记录所有回退逻辑位置
- [ ] **实现纯磁盘查询**:
  ```typescript
  // persistentStore.ts - 需要修改的方法
  async query(pattern: Partial<Triple>, options?: QueryOptions): Promise<FactRecord[]> {
    if (this.pinnedEpochStack.length > 0) {
      // TODO: 完全依赖 PagedIndexReader，移除内存回退
      const pinnedEpoch = this.pinnedEpochStack[this.pinnedEpochStack.length - 1];
      return await this.queryFromDisk(pattern, pinnedEpoch, options);
    }
    // ... 现有逻辑
  }
  ```
- [ ] **创建专用磁盘查询方法**: `queryFromDisk()`
  - 输入: pattern, epoch, options
  - 输出: FactRecord[]
  - 要求: 零内存缓存，完全流式

#### B.1.2 增强 GC 的 Epoch 感知能力

- [ ] **修改 GC 逻辑**: `garbageCollectPages()` 需要检查活跃读者

  ```typescript
  // gc.ts - 新增检查
  async garbageCollectPages(options: GCOptions): Promise<GCResult> {
    // TODO: 检查 readers 目录
    const activeReaders = await this.getActiveReaders();
    const pinnedEpochs = activeReaders.map(r => r.epoch);

    // 只回收不被引用的页
    const safeToDelete = orphans.filter(page =>
      !pinnedEpochs.includes(page.epoch)
    );
  }
  ```

- [ ] **实现读者追踪**: 增强 `ReaderRegistry`
  - 新增方法: `getActiveEpochs(): number[]`
  - 新增方法: `isEpochInUse(epoch: number): boolean`

#### B.1.3 创建验收测试

- [ ] **大数据集快照测试**: `tests/snapshot_memory_optimization.test.ts`
  ```typescript
  it('快照查询不增加内存占用', async () => {
    // 创建 10,000+ 条记录
    // 记录初始内存
    // 启动 withSnapshot 查询
    // 并发执行 compact 和 gc
    // 断言内存增长 < 10MB
    // 验证查询结果正确性
  });
  ```
- [ ] **性能基准测试**: 对比优化前后的内存和速度

---

### 📝 B.2: 实现属性过滤下推 (Predicate Pushdown)

**预估工时**: 7天
**优先级**: 🔴 高
**影响文件**:

- `src/storage/propertyStore.ts` (新增索引)
- `src/storage/persistentStore.ts` (更新写路径)
- `src/query/queryBuilder.ts` (新增 API)
- `src/storage/layout.ts` (新增索引文件路径)

#### B.2.1 创建属性倒排索引

- [ ] **设计索引结构**:
  ```typescript
  // propertyIndex.ts - 新文件
  interface PropertyIndex {
    nodeProperties: Map<string, Map<any, Set<number>>>; // 属性名 -> 值 -> nodeIds
    edgeProperties: Map<string, Map<any, Set<string>>>; // 属性名 -> 值 -> edgeKeys
  }
  ```
- [ ] **实现索引持久化**:
  - 文件位置: `.pages/properties.idx`
  - 序列化格式: 分页 B+树结构
  - 支持增量更新

#### B.2.2 更新写入路径

- [ ] **修改属性设置方法**:

  ```typescript
  // persistentStore.ts
  async setNodeProperties(nodeId: number, props: any): Promise<void> {
    // 现有逻辑...

    // TODO: 更新属性索引
    await this.propertyIndex.indexNodeProperty(nodeId, props);
  }
  ```

- [ ] **实现索引更新钩子**:
  - 新增属性时: 添加到索引
  - 更新属性时: 先删除旧索引，再添加新索引
  - 删除节点时: 清理相关索引

#### B.2.3 增强 QueryBuilder API

- [ ] **新增结构化 where API**:
  ```typescript
  // queryBuilder.ts
  whereProperty(
    path: 'subject' | 'object',
    property: string,
    operator: '=' | '!=' | '>' | '<' | 'in',
    value: any
  ): this {
    // TODO: 记录过滤条件
    this.propertyFilters.push({ path, property, operator, value });
    return this;
  }
  ```
- [ ] **实现查询优化器**:
  - 识别可下推的过滤条件
  - 先查询属性索引获取 ID 集合
  - 使用 ID 集合过滤主查询

#### B.2.4 性能验收测试

- [ ] **创建性能对比测试**: `tests/property_pushdown_perf.test.ts`
  ```typescript
  it('属性索引查询性能提升 10x+', async () => {
    // 创建 10,000 个节点，10 个符合条件
    // 测试1: 使用传统 where 过滤
    // 测试2: 使用 whereProperty 索引
    // 断言: T2 < T1 / 10
  });
  ```

---

### 📝 B.3: 增强可观测性 (Observability)

**预估工时**: 3天
**优先级**: 🟡 中
**影响文件**:

- `src/cli/commands/stats.ts`
- `src/cli/commands/autoCompact.ts`
- `src/cli/commands/readers.ts` (新文件)

#### B.3.1 丰富 db:stats 命令

- [ ] **增加读者统计**:
  ```typescript
  stats.readers = {
    active: activeReaders.length,
    epochs: activeReaders.map((r) => r.epoch),
    oldest: Math.min(...epochs),
  };
  ```
- [ ] **增加索引大小统计**:
  ```typescript
  for (const order of ['SPO', 'POS', 'OSP']) {
    stats.orders[order].diskSize = await getFileSize(indexPath);
    stats.orders[order].pageCount = index.pageCount;
  }
  ```
- [ ] **增加 WAL 统计**:
  ```typescript
  stats.wal = {
    pendingRecords: wal.getPendingCount(),
    fileSize: await getFileSize(walPath),
    lastFlush: wal.getLastFlushTime(),
  };
  ```

#### B.3.2 增强 auto-compact 日志

- [ ] **详细决策日志**:
  ```typescript
  // autoCompact.ts
  console.log(`Compaction Decision:
    Primary: ${primary}
    Score: ${score.total}
    - Hotness: ${score.hotness}
    - Page Count: ${score.pageCount}
    - Fragmentation: ${score.fragmentation}
    Action: ${decision.action}
    Reason: ${decision.reason}
  `);
  ```

#### B.3.3 新增 db:readers 命令

- [ ] **创建命令文件**: `src/cli/commands/readers.ts`
  ```typescript
  export async function readersCommand(dbPath: string) {
    const readers = await getActiveReaders(dbPath);
    console.table(
      readers.map((r) => ({
        PID: r.pid,
        Epoch: r.epoch,
        Started: r.startTime,
        Duration: Date.now() - r.startTime,
      })),
    );
  }
  ```
- [ ] **注册到 CLI**: 更新 `src/cli/nervusdb.ts`

---

## 🚀 里程碑 C: V1.0 正式版功能 (P2 - 中低优先级)

### 📝 C.1: 类型安全 API (Type-Safe API)

**预估工时**: 4天
**优先级**: 🟡 中
**影响文件**:

- `src/synapseDb.ts`
- `src/query/queryBuilder.ts`
- `src/types/index.ts`

#### C.1.1 泛型化核心类

- [ ] **重构 NervusDB 类**:
  ```typescript
  // synapseDb.ts
  export class NervusDB<
    TNode extends Record<string, any> = any,
    TEdge extends Record<string, any> = any,
  > {
    // 所有方法签名需要更新
  }
  ```
- [ ] **重构 QueryBuilder 类**:
  ```typescript
  // queryBuilder.ts
  export class QueryBuilder<TNode, TEdge> {
    where(predicate: (record: FactRecord<TNode, TEdge>) => boolean): this;
  }
  ```

#### C.1.2 更新 API 签名

- [ ] **addFact 方法**:
  ```typescript
  addFact(
    fact: Triple,
    options?: {
      subjectProperties?: Partial<TNode>;
      objectProperties?: Partial<TNode>;
      edgeProperties?: Partial<TEdge>;
    }
  ): void
  ```
- [ ] **属性获取方法**:
  ```typescript
  getNodeProperties(nodeId: number): TNode | null
  getEdgeProperties(edgeKey: string): TEdge | null
  ```

#### C.1.3 创建类型安全示例

- [ ] **示例测试**: `tests/type_safety.test.ts`
- [ ] **文档示例**: `docs/examples/typescript_types.md`

---

### 📝 C.2: 高级查询优化器 (Advanced Query Optimizer)

**预估工时**: 6天
**优先级**: 🟢 低
**影响文件**:

- `src/query/queryOptimizer.ts` (新文件)
- `src/query/queryBuilder.ts`
- `src/maintenance/compaction.ts` (统计收集)

#### C.2.1 统计信息收集

- [ ] **设计统计结构**:
  ```typescript
  interface QueryStats {
    predicateCardinality: Map<string, number>; // 谓词基数
    nodeSelectivity: Map<string, number>; // 节点选择性
    avgFanout: Map<string, number>; // 平均扇出
  }
  ```
- [ ] **在压缩时收集**: 修改 `compaction.ts`
- [ ] **持久化到文件**: `stats.json`

#### C.2.2 查询计划生成

- [ ] **创建查询计划树**:
  ```typescript
  interface QueryPlan {
    type: 'scan' | 'index' | 'join';
    cost: number;
    children?: QueryPlan[];
  }
  ```
- [ ] **实现成本估算**: 基于统计信息

#### C.2.3 查询重写优化

- [ ] **实现查询重写规则**:
  - 谓词下推
  - 连接重排序
  - 索引选择
- [ ] **创建 EXPLAIN 命令**: 展示查询计划

---

## 📈 实施进度跟踪

### 第一阶段（1-2周）：技术债务 + B.1 ✅ **已完成**

- [x] B.0.1: 修复 FileHandle 警告
- [x] B.1.1: 重构快照查询机制
- [x] B.1.2: 增强 GC Epoch 感知
- [x] B.1.3: 创建验收测试
- [x] B.1.4: 优化 pagedIndex.ts 批量写入性能
- [x] B.1.5: 修复查询逻辑中内存数据合并问题

### 第二阶段（2-3周）：B.2 属性索引 ✅ **已完成**

- [x] B.2.1: 创建属性倒排索引
- [x] B.2.2: 更新写入路径
- [x] B.2.3: 增强 QueryBuilder API
- [x] B.2.4: 性能验收测试

### 第三阶段（1周）：B.3 可观测性 ✅ **已完成**

- [x] B.3.1: 丰富 db:stats
- [x] B.3.2: 增强 auto-compact 日志
- [x] B.3.3: 新增 db:readers 命令

### 第四阶段（2周）：C.1 + C.2

- [ ] C.1: 类型安全 API
- [ ] C.2: 查询优化器（可选）

---

## 🎯 验收标准

### Beta 版本验收（里程碑 B 完成）

1. **性能指标**:
   - 大数据集（10万+记录）查询内存占用 < 50MB
   - 属性过滤查询性能提升 10x+
   - 无 FileHandle 泄漏警告

2. **功能完整性**:
   - 所有现有测试通过
   - 新增测试覆盖率 > 90%
   - CLI 工具功能完善

### 正式版验收（里程碑 C 完成）

1. **开发体验**:
   - TypeScript 类型推断完整
   - API 文档完善
   - 示例代码丰富

2. **生产就绪**:
   - 查询优化器有效
   - 监控指标完善
   - 压力测试通过

---

## 📚 参考资源

- [Roadmap v2.0](/docs/项目发展路线图/Roadmap%20v2.0.md)
- [架构设计文档](/docs/architecture.md)
- [API 文档](/docs/api.md)
- [性能测试报告](/docs/performance.md)

---

## 🔄 更新记录

- 2025-01-24: 初始版本，基于 Roadmap v2.0 创建
- 2025-09-24: **B 阶段完成** - 所有优化与增强任务已实现并通过测试
  - ✅ B.0: 技术债务清理（FileHandle 修复）
  - ✅ B.1: 内存优化（快照隔离、GC 增强、性能优化）
  - ✅ B.2: 属性倒排索引（完整实现，支持 O(log n) 查询）
  - ✅ B.3: 可观测性改进（增强 CLI 工具、详细日志）
