当然。在你成功完成了所有测试修复和关键的架构重构之后，项目已经达到了一个新的高度。现在是时候将目光投向未来，规划如何将这个坚固的 Beta 版本打磨成一个功能更丰富、性能更卓越的 1.0 正式版。

基于所有AI模型的综合审查报告，我为你整理了一份详细、可执行的**SynapseDB 后续开发路线图**。这份路线图将P1（Beta强化）和P2（正式版功能）的设想具体化为可操作的工程任务。

---

## SynapseDB 发展路线图 (Roadmap v2.0)

**当前状态**: v1.0-beta. 高质量、功能完备、具备核心可伸缩性的数据库引擎。所有已知Bug已修复，关键性能瓶颈已通过架构重构解决。

**路线图目标**: 将 SynapseDB 从一个健壮的 Beta 版本，演进为一个在**性能、查询能力、开发者体验和可观测性**上都达到工业级标准的 v1.0 正式版。

---

### **里程碑 B: “Beta” 强化与效率调优 (P1)**

**目标：将引擎打磨至可在生产环境小规模部署，并显著提升查询效率与可维护性。**

#### **【任务 B.1】优化快照隔离的内存占用**

- **优先级**: **高**
- **目标**: 解除快照查询对内存 `TripleStore` 的依赖，使其在处理大型数据集时也能保持极低的内存占用，真正实现从磁盘读取。
- **实现思路**:
  1.  **修改 `PersistentStore.query()`**: 当 `pinnedEpochStack` 不为空（即处于快照中）时，查询逻辑应**完全**依赖于为该 `epoch` 创建的 `PagedIndexReader` 实例进行磁盘读取。移除所有回退到内存 `TripleStore` 的逻辑。
  2.  **增强垃圾回收 (`GC`) 逻辑**:
      - `garbageCollectPages` 需要变得“epoch感知”。它在决定是否删除一个旧的页文件（orphans）之前，必须检查 `readers` 目录，确认没有任何活跃的读者/快照仍然固定在该旧 `epoch` 上。
      - 只有当一个页版本既不被当前 `manifest` 引用，也不被任何活跃快照引用时，它才能被安全地回收。
- **验收标准**:
  - 编写一个新的集成测试：创建一个大型数据库（> 10,000条记录），启动一个 `withSnapshot` 查询，在查询中途（`sleep`）并发执行一次 `auto-compact` 和一次 `gc`。
  - 验证快照查询结果仍然正确无误。
  - 在整个测试期间，通过 `process.memoryUsage()` 监控内存，断言其增长量远小于数据库文件的尺寸。
  - 快照结束后，再次运行 `gc`，验证磁盘空间被成功回收。

#### **【任务 B.2】实现属性过滤下推 (Predicate Pushdown)**

- **优先级**: **高**
- **目标**: 大幅提升带有高选择性属性过滤的 `where` 查询的性能，避免全量数据读取和内存过滤。
- **实现思路**:
  1.  **引入属性索引**: 创建一个新的存储结构（例如，在 `.pages` 目录下创建 `properties.idx` 文件），用于存储属性的倒排索引。
      - 数据结构可以是一个 `Map<string, Map<any, number[]>>`，即 `Map<属性名, Map<属性值, 节点ID列表>>`。
  2.  **更新写路径**: 修改 `PersistentStore.setNodeProperties` 和 `setEdgeProperties`，在写入属性的同时，异步更新这个倒排索引。
  3.  **增强 `QueryBuilder`**:
      - 新增一个专门的API，如 `.whereProperty('type', '=', 'File')`。这种结构化的API允许查询引擎明确地识别出可下推的过滤条件。
      - 修改 `QueryBuilder` 的内部逻辑。当遇到 `whereProperty` 时，查询计划会改变：
        a. 首先查询属性索引，获取一个小的、符合条件的 `nodeId` 集合。
        b. 然后，在执行 `find` 或 `follow` 时，将这个 `nodeId` 集合作为额外的过滤条件，大大减少需要从主索引中读取和处理的数据量。
- **验收标准**:
  - 编写一个新的性能测试：创建一个包含10,000个节点，其中只有10个节点的属性为 `status: 'active'` 的数据库。
  - 执行 `.find({}).where(r => r.subjectProperties.status === 'active')` 并记录时间 `T1`。
  - 执行 `.find({}).whereProperty('status', '=', 'active')` 并记录时间 `T2`。
  - 断言 `T2` 远小于 `T1`（至少一个数量级）。

#### **【任务 B.3】增强可观测性 (Observability)**

- **优先级**: **中**
- **目标**: 为开发者和运维人员提供更深入的数据库内部状态洞察，使性能调优和问题诊断变得简单。
- **实现思路**:
  1.  **丰富 `db:stats`**:
      - 增加 `readers` 字段，显示当前活跃读者的数量。
      - 在 `orders` 统计中，增加每个索引文件的磁盘大小。
      - 增加 `wal` 统计，显示待处理记录的数量。
  2.  **增强 `db:auto-compact`**: 在执行后，打印出本次决策的详细摘要，例如：“Compacted SPO on primary 42 (Score: 15.5, Reason: High Hotness=12, Page Count=4)”。
  3.  **新增 `db:readers` 命令**: 创建一个新的CLI命令，用于列出所有当前活跃的读者进程ID、它们固定的 `epoch` 以及启动时间。
- **验收标准**:
  - 运行所有更新后的CLI命令，验证其输出内容翔实、准确且格式清晰。
  - 在有活跃 `withSnapshot` 查询时运行 `db:readers`，能够看到对应的读者信息。

---

### **里程碑 C: “V1.0 正式版”功能与生态 (P2)**

**目标：提供更高级的数据库特性和一流的开发者体验，为正式发布做准备。**

#### **【任务 C.1】类型安全 API (Type-Safe API)**

- **优先级**: **中**
- **目标**: 利用TypeScript的泛型系统，允许用户定义自己的节点和边属性的类型，并在应用层代码中获得编译时类型检查和智能提示。
- **实现思路**:
  1.  **泛型化 `SynapseDB` 和 `QueryBuilder`**:
      - `class SynapseDB<TNode extends {}, TEdge extends {}>`
      - `class QueryBuilder<TNode extends {}, TEdge extends {}>`
  2.  **更新 API 签名**:
      - `addFact(fact, { subjectProperties?: Partial<TNode>, ... })`
      - `getNodeProperties(nodeId: number): TNode | null`
      - `where(predicate: (record: FactRecord<TNode, TEdge>) => boolean)`
- **验收标准**:
  - 创建一个新的示例测试文件 (`type-safety.test.ts`)。
  - 在测试中定义 `interface User { name: string; age: number; }`。
  - 打开数据库 `SynapseDB.open<User, {}>()`。
  - 调用 `addFact` 时，如果传入 `{ subjectProperties: { name: 'Alice', age: 'twenty' } }`（age类型错误），TypeScript编译器应报错。
  - `where` 回调中的 `record` 参数应能被IDE正确推断为 `FactRecord<User, {}>` 类型。

#### **【任务 C.2】高级查询优化器 (Advanced Query Optimizer)**

- **优先级**: **低**
- **目标**: 对于包含多个 `follow` 的复杂链式查询，能够基于数据统计信息选择最优的执行路径。
- **实现思路**:
  1.  **统计信息收集**: 在 `compaction` 过程中，收集并持久化关于数据的统计信息，例如每个谓词（predicate）的“基数”（cardinality，即唯一关系的数量）。这些信息可以存储在 `manifest` 或一个专门的 `stats.json` 文件中。
  2.  **查询计划生成**: 在 `QueryBuilder` 链的末端（例如调用 `.all()` 或开始迭代时），不是立即执行，而是先生成一个简单的查询计划树。
  3.  **成本估算与重排**: 查询优化器会分析这个计划树，并根据存储的统计信息估算不同执行顺序（例如，从头开始 `follow` vs 从尾部 `followReverse`）的成本。然后选择成本最低的计划来执行。
- **验收标准**:
  - 创建一个非对称图的性能测试：`A --(many_links)--> B --(few_links)--> C`。
  - 执行查询 `db.find({subject: 'A'}).follow('many_links').follow('few_links')`。
  - 分析（通过日志或新的 `db:query:explain` 命令）查询计划，验证优化器选择了从 C 开始反向查询的更优路径。

---

通过完成这些里程碑，SynapseDB 将不仅是一个健壮可靠的数据库，更将成为一个在性能、易用性和智能性上都具备强大竞争力的顶级嵌入式知识图谱解决方案。
