# NervusDB 差距分析：距离"图数据库的 SQLite"还有多远

> 写于 2026-03-14，基于对当前代码库的完整审查。
> 目的：诚实记录现状与目标之间的差距，供后续迭代参考。

---

## 当前已有什么

一个能跑的单文件嵌入式存储引擎原型，具备：

- 单文件 Pager + Bitmap 页面管理
- WAL 崩溃恢复
- BTree 索引（insert / delete / cursor）
- CSR 压缩边存储
- IdMap 节点映射
- HNSW 向量索引（BETA-05 阶段，正在修 bug）
- 基本事务语义（begin / commit）
- Bulk loader
- Vacuum
- C ABI v1 + Node / Python thin bindings

这大概相当于 SQLite 在 2000 年 D. Richard Hipp 刚写出第一版时的状态。

---

## 六大差距

### 1. 查询语言 — 完全缺失（工作量最大）

SQLite 有完整的 SQL 解析器、查询规划器、字节码虚拟机。
NervusDB 目前没有任何声明式查询语言，用户只能通过 Rust API 手写 `create_node` / `begin_write`。

**没有查询语言的图数据库 = 只是一个存储引擎，不是数据库。**

目标：实现一个最小可用的 Cypher 子集（MATCH / CREATE / WHERE / RETURN）。
工作量估计：约等于当前已写代码量的 3-5 倍。

参考路径：
- `docs/design/T300-cypher-full.md` 已有设计草稿
- `docs/cypher-support.md` 有支持范围规划
- 建议分三阶段：Lexer → Parser → Executor，每阶段独立可测试

---

### 2. 查询优化器 — 完全缺失

图遍历的查询计划（BFS/DFS 选择、索引选择、join 顺序、短路优化）是图数据库的核心竞争力。
Neo4j 在这上面投了十几年。

当前读路径是硬编码的迭代器链，没有任何 cost-based 优化。

目标：
- 第一步：基于规则的优化器（RBO），覆盖最常见的索引选择和谓词下推
- 第二步：统计信息收集（节点/边数量、属性分布）
- 第三步：基于代价的优化器（CBO）

---

### 3. 并发控制 — 粗粒度锁，不够用

SQLite WAL 模式支持多读单写并发。
NervusDB 当前是 `RwLock<Pager>` + `Mutex<HnswIndex>`，本质上是全局大锁。

对嵌入式单进程场景勉强够用，但：
- 读操作会阻塞写操作
- HNSW 搜索会阻塞所有其他操作
- 无法支持多线程并发读

目标：实现 SQLite 级别的 WAL 并发模型（多读单写，读不阻塞写）。
参考：`docs/design/T205-pager-lock-granularity.md`

---

### 4. 正确性与稳定性 — 差距最大，需要多年积累

SQLite 的测试覆盖率是代码量的 **600 倍**（没写错）。
它有 TH3、dbsqlfuzz、OSSFuzz 持续跑。

NervusDB 当前状态：
- 基础的 I2E 页面冲突（512 节点以上）直到 BETA-05 才被发现
- 没有 fuzz testing
- 没有 chaos / crash injection 测试框架（crash-test 工具刚起步）
- 测试覆盖率估计 < 30%

目标路径：
1. 先把已知 bug 修完（当前 BETA-05 阶段）
2. 引入 cargo-fuzz 对 Pager / BTree / BlobStore 做 fuzz
3. 扩展 crash-test 工具，覆盖更多崩溃场景
4. 建立 regression test suite，每个 bug 修复后必须有对应测试

**嵌入式数据库的信任是用年为单位积累的。**

---

### 5. 跨语言生态 — 刚起步

SQLite 的杀手锏是"任何语言都能用"。
NervusDB 有 C ABI v1 和 Node/Python thin bindings，但：

- 没有完整的 SDK 文档
- 没有错误处理的最佳实践指南
- 没有连接池 / 异步支持
- 没有 ORM 集成

目标：
- 完善 Node.js SDK（async/await 原生支持）
- 完善 Python SDK（with 语句、类型提示）
- 发布到 npm / PyPI
- 提供 Go / Java / C# bindings（长期）

---

### 6. 工程打磨细节 — 差距最明显的地方

| 功能 | SQLite | NervusDB 当前 |
|------|--------|--------------|
| 运行时配置 | `PRAGMA` 机制 | 环境变量，不完整 |
| 调试工具 | `EXPLAIN QUERY PLAN` | 无 |
| 在线热备 | `.backup` API | Vacuum（不等价） |
| Page cache | LRU buffer pool | 无，每次直接 I/O |
| 最大数据库大小 | 281 TB | 512 MB（bitmap 限制） |
| Schema 版本管理 | `user_version` PRAGMA | `storage_format_epoch`（粗粒度） |
| 只读模式 | 支持 | 不支持 |
| 内存模式 | `:memory:` | 不支持 |

**最紧迫的工程债**：
- Bitmap 只支持 65536 页（512 MB 上限）→ 需要多级 bitmap 或 freelist
- 没有 page cache / buffer pool → 大数据集性能会很差
- `EXPLAIN` 工具缺失 → 用户无法诊断查询性能

---

## 里程碑估计

| 阶段 | 当前完成度 | 到 SQLite 级别需要 |
|------|-----------|-------------------|
| 存储引擎 | ~70% | 修完当前 bug，加 buffer pool，扩展 bitmap |
| 事务/并发 | ~30% | MVCC 或 SQLite 级 WAL 并发 |
| 查询语言 | ~5% | Lexer + Parser + Planner + VM |
| 查询优化 | ~0% | Cost model + Statistics + Plan cache |
| 跨语言生态 | ~15% | 完整 SDK + 文档 + 包发布 |
| 测试/稳定性 | ~10% | Fuzz testing + 多年打磨 |
| 文档/社区 | ~5% | 用户文档 + 示例 + 社区建设 |

---

## 好消息

这个赛道（嵌入式图数据库）目前**没有真正的 SQLite 级选手**：

- DuckDB 是关系型，不是图
- RocksDB 是 KV，不是图
- Neo4j / ArangoDB 是服务端，不是嵌入式
- Kuzu 最接近，但向量搜索能力弱

**如果 NervusDB 能把查询语言做出来，哪怕是最小的 Cypher 子集，就已经比市面上大多数嵌入式图存储领先了。**

前提是先把地基打牢——也就是当前 BETA-05 正在做的事情。

---

## 下一步优先级建议

1. **P0（当前）**：修完 I2E 页面冲突 bug，让 Nightly 转绿
2. **P1（BETA-05 收敛后）**：Buffer pool / Page cache（没有它，大数据集性能无法接受）
3. **P2**：Bitmap 扩容（突破 512 MB 限制）
4. **P3**：最小 Cypher 子集（这是从"存储引擎"变成"数据库"的关键跨越）
5. **P4**：WAL 并发优化（多读单写）
6. **长期**：查询优化器、完整 Cypher、跨语言 SDK 生态
