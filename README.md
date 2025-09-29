# SynapseDB

> 嵌入式三元组（SPO）知识库，服务本地/边缘场景的知识管理、链式联想与轻量推理。

## 目录

1. [项目概览](#项目概览)
2. [系统架构总览](#系统架构总览)
3. [核心特性矩阵](#核心特性矩阵)
4. [安装与环境准备](#安装与环境准备)
5. [快速上手](#快速上手)
6. [数据模型与 API](#数据模型与-api)
7. [存储格式与持久化](#存储格式与持久化)
8. [查询引擎与多语言接口](#查询引擎与多语言接口)
9. [事务、并发与容错机制](#事务并发与容错机制)
10. [索引体系与性能调优](#索引体系与性能调优)
11. [运维与治理工具](#运维与治理工具)
12. [监控、诊断与可观察性](#监控诊断与可观察性)
13. [集成模式与实践](#集成模式与实践)
14. [示例与学习资源](#示例与学习资源)
15. [安全、备份与合规](#安全备份与合规)
16. [路线图与版本策略](#路线图与版本策略)
17. [常见问题 FAQ](#常见问题-faq)
18. [参与贡献与支持](#参与贡献与支持)
19. [许可证](#许可证)

---

## 项目概览

- **定位**：SynapseDB 是一个单机嵌入式知识库，核心围绕“事实三元组（subject/predicate/object）+ 属性”存储和链式联想查询。以 TypeScript/Node.js 实现，强调 _可恢复、可治理、可扩展_。
- **运行环境**：Node.js ≥ 18（推荐 20/22），macOS / Linux / Windows 均可。仓库采用 ESM，代码、注释与文档全部中文。
- **适用场景**：代码知识图谱、配置依赖图、嵌入式 AI 记忆体、数据血缘审计、DevOps 知识库、轻量边缘部署、离线推理/召回。
- **技术现状**：主干版本 v1.1.x；WAL v2、增量/整序 compaction、链式联想、QueryBuilder/Cypher/GraphQL/Gremlin、多属性索引、全文检索、空间索引、图算法、自动化治理 CLI 均已稳定。
- **近期验证（2025-09-28）**：
  - ✅ `pnpm test`（132 个测试文件，629 个用例：628 通过 / 1 跳过）
  - ✅ `repomix__pack_codebase` 输出 ID `fe8ec9a35849d45e`（440 文件，842,228 tokens）

## 系统架构总览

```
┌──────────────────────────────────────────────┐
│                 应用 / 工具层                 │
│  - Node.js 应用 / CLI / 脚本 / MCP / VSCode  │
│  - GraphQL / Gremlin / Cypher 适配层          │
└───────────────▲──────────────────────────────┘
                │ QueryBuilder 链式接口
┌───────────────┴──────────────────────────────┐
│                查询与执行引擎                 │
│  - 链式联想管线          - 聚合/流式迭代器       │
│  - 多语言解析器          - 图算法/属性过滤       │
└───────────────▲──────────────────────────────┘
                │ Storage API
┌───────────────┴──────────────────────────────┐
│                   存储内核                    │
│  - 六序索引 (SPO/SOP/POS/PSO/OSP/OPS)         │
│  - WAL v2 / 事务批次 / 读快照 (epoch-pin)      │
│  - 属性/全文/空间索引模块                     │
│  - 热度统计、读者注册、LSM-Lite 暂存           │
└───────────────▲──────────────────────────────┘
                │ fs 底层
┌───────────────┴──────────────────────────────┐
│                 持久化文件布局                 │
│  - <db>.synapsedb                              │
│  - <db>.synapsedb.pages/                       │
│  - <db>.synapsedb.wal                          │
└──────────────────────────────────────────────┘
```

## 核心特性矩阵

| 能力       | 子模块                                            | 说明                                                                 |
| ---------- | ------------------------------------------------- | -------------------------------------------------------------------- |
| 三元组存储 | `src/storage/tripleStore.ts`                      | 单文件主库 + 六序分页索引，按主键排序分页                            |
| 链式查询   | `src/query/queryBuilder.ts`                       | 正反向 follow、anchor、属性过滤、Streaming、聚合                     |
| 多语言接口 | `src/query/cypher.ts` / `graphql/*` / `gremlin/*` | Cypher、GraphQL、Gremlin 与 QueryBuilder 共用执行管线                |
| 事务与 WAL | `src/storage/wal.ts` / `txidRegistry.ts`          | WAL v2、批次提交、崩溃恢复、事务 ID 幂等                             |
| 热度与治理 | `src/storage/hotness.ts` / `src/maintenance/*`    | 热度统计、自动压实、GC、页修复、读者尊重策略                         |
| 属性索引   | `src/storage/propertyIndex.ts`                    | 节点/边属性倒排索引，范围/前缀/精确匹配                              |
| 全文检索   | `src/fulltext/*`                                  | 分词、倒排索引、打分器、批处理导入、查询 DSL                         |
| 空间索引   | `src/spatial/*`                                   | R-Tree、几何类型、范围/相交/最近邻查询                               |
| 图算法     | `src/algorithms/*`                                | Dijkstra、A\*、双向 BFS、中心性、社区发现、相似度                    |
| 基准测试   | `benchmarks/*` ⚠️ `src/benchmark/*` 已弃用        | 外部脚本化基准套件，输出详细指标；CLI 保持兼容但推荐直接运行脚本     |
| CLI 工具   | `src/cli/*`                                       | stats、check、repair、compact、auto-compact、gc、hot、txids、dump 等 |

## 安装与环境准备

### 依赖

- Node.js 18+（推荐 20+）
- pnpm 8/9/10（推荐 10）
- macOS/Linux/Windows 均通过 CI 验证

### 安装流程

```bash
pnpm install              # 安装依赖
pnpm build                # TypeScript -> dist/
npm i -g .                # 可选：全局安装 CLI（生成 synapsedb 命令）
```

### 其他选择

- **包管理器**：`npm install synapsedb` 或 `yarn add synapsedb`
- **Bun**：`bun add synapsedb`
- **Docker**（只读示例）：社区镜像见 `docs/教学文档/教程-08-部署与最佳实践.md`
- **repl/脚本化**：`pnpm dlx tsx scripts/dump-graph.mjs`

> 建议配置 `PNPM_HOME` 并执行 `pnpm setup`，确保全局命令可用。

## 快速上手

### 1. 准备示例库

```bash
synapsedb bench demo.synapsedb 200 lsm   # 生成 200 条演示数据，启用 LSM 暂存
synapsedb stats demo.synapsedb           # 查看基本统计
```

### 2. Node.js 程序调用

```ts
import { SynapseDB } from 'synapsedb';

const db = await SynapseDB.open('demo.synapsedb', {
  enableLock: true,
  registerReader: true,
  enablePersistentTxDedupe: true,
});

db.addFact(
  { subject: 'user:alice', predicate: 'FRIEND_OF', object: 'user:bob' },
  {
    subjectProperties: { labels: ['Person'], dept: 'R&D' },
    objectProperties: { labels: ['Person'], dept: 'Ops' },
    edgeProperties: { since: '2024-06-01', strength: 0.82 },
  },
);

const chain = await db
  .find({ subject: 'user:alice', predicate: 'FRIEND_OF' })
  .follow('FRIEND_OF')
  .limit(5)
  .all();

await db.flush();
await db.close();
```

### 3. 类型安全封装

```ts
import { TypedSynapseDB } from '@/typedSynapseDb';

interface PersonNode {
  labels: string[];
  title?: string;
}
interface RelationEdge {
  since: string;
  strength: number;
}

const social = await TypedSynapseDB.open<PersonNode, RelationEdge>('social.synapsedb');
const good = await social
  .find({ predicate: 'FRIEND_OF' })
  .where((edge) => edge.edgeProperties?.strength! > 0.75)
  .limit(20)
  .all();
```

### 4. CLI 体验

```bash
synapsedb auto-compact demo.synapsedb \
  --mode=incremental --hot-threshold=1.2 --max-primary=5 --auto-gc
synapsedb txids demo.synapsedb --list=20
synapsedb dump demo.synapsedb SPO 1
```

更多场景见 [示例与学习资源](#示例与学习资源)。

## 数据模型与 API

### 事实三元组

- **主体 (subject)**：实体或资源，内部映射为 `subjectId`
- **谓词 (predicate)**：关系类型，内部映射为 `predicateId`
- **客体 (object)**：目标实体或值，内部映射为 `objectId`
- 事实写入：`addFact({ subject, predicate, object }, { subjectProperties?, objectProperties?, edgeProperties? })`

### 属性系统

- 节点属性存储：`subjectId` / `objectId` -> JSON 文档，带版本号 `__v`
- 边属性存储：`subjectId:predicateId:objectId` -> JSON
- 属性索引：支持精确、范围、前缀、集合包含查询，详见 `docs/使用示例/TypeScript类型系统使用指南.md`

### QueryBuilder API

```ts
const res = await db
  .find({ predicate: 'FRIEND_OF' }, { anchor: 'subject' })
  .where((edge) => (edge.edgeProperties?.strength as number) > 0.6)
  .follow('WORKS_AT')
  .followReverse('MANAGED_BY')
  .limit(10)
  .all();
```

- `follow` / `followReverse`
- `anchor`: `'subject' | 'object' | 'both'`
- `where` 支持属性/节点/边上下文
- `limit` / `takeUntil` / `distinct`
- Streaming：`for await (const batch of db.streamFacts(criteria, batchSize)) {}`
- 聚合：`db.aggregate().match(...).groupBy(...).count(...).execute()`

### 批次与事务

- `beginBatch({ txId?, sessionId? })`
- `commitBatch({ durable?: boolean })`
- `abortBatch()`
- `txId` + `sessionId` 实现跨周期幂等去重（重放时跳过重复提交）

更多 API 定义见 `src/types/openOptions.ts` 与 `docs/教学文档/教程-02-数据模型与基础CRUD.md`。

## 存储格式与持久化

```
brain.synapsedb               # 主数据文件
  ├─ 64B Header (魔数 SYNAPSEDB, version=2)
  ├─ dictionary section       # 字符串 <-> ID 映射
  ├─ triples section          # 主体/谓词/客体按主键排序
  ├─ indexes (staging)        # LSM-Lite 暂存段（可选）
  └─ properties section       # 节点/边属性 JSON

brain.synapsedb.pages/        # 分页索引目录
  ├─ SPO.idxpage / SOP.idxpage / ...      # 各顺序页文件
  ├─ index-manifest.json                  # 页映射、epoch、tombstones、orphans
  ├─ hotness.json                         # primary 热度计数 + 半衰
  ├─ readers.json                         # 活跃读者信息
  └─ txids.json                           # 幂等事务注册表（可选）

brain.synapsedb.wal           # WAL v2（追加写）
```

特性说明：

- **WAL**：记录 add/delete/property/txId；崩溃恢复时重放并在 safe offset 截断。
- **Manifest 原子更新**：先写 `*.tmp` + fsync，再原子 rename，避免脏文件。
- **热度统计**：按访问/写入自衰减，驱动自动压实。
- **读者注册**：治理时检测活跃快照，自动跳过有读者的主键。

详细文件结构见 `docs/教学文档/教程-07-存储格式与持久化.md`。

## 查询引擎与多语言接口

### QueryBuilder

- 起点：任意 subject/predicate/object 组合
- 前沿去重：基于 `subjectId:predicateId:objectId`
- 支持 streaming 输出、批量聚合、属性下推
- `withSnapshot(fn)` 在回调期间固定 manifest epoch

### Cypher

```cypher
MATCH (p:Person)-[r:FRIEND_OF*1..3]->(q)
WHERE r.weight > 0.6 AND q.labels CONTAINS 'Engineer'
RETURN q LIMIT 20;
```

- 语法子集详见 `docs/使用示例/Cypher语法参考.md`
- 实现：`src/query/cypher.ts`

### GraphQL

```graphql
query Friends($id: ID!) {
  synapse {
    find(subject: $id, predicate: "FRIEND_OF") {
      subject
      object
      edgeProperties
    }
  }
}
```

- 参阅 `docs/使用示例/graphql_usage.md`

### Gremlin

```javascript
g.V('user:alice').repeat(out('FRIEND_OF')).times(2).values('dept');
```

- 参阅 `docs/使用示例/gremlin_usage.md`

### 图算法

- Dijkstra / A\*：最短路径、权重字段支持
- 双向 BFS：快速最短跳数
- 社区发现 (Louvain) & 中心性 (PageRank/Betweenness)
- 相似度：Jaccard、余弦
- 入口文件：`src/algorithms/*`，示例见 `docs/使用示例/图算法库使用指南.md`

### 全文与空间

- 全文：自带分词、倒排索引、批处理导入与打分器，见 `docs/使用示例/全文搜索使用指南.md`
- 空间：R-Tree + 几何类型，支持范围、相交、最近邻，见 `docs/使用示例/空间几何计算指南.md`

## 事务并发与容错机制

- **WAL v2**：`BEGIN/COMMIT/ABORT`，崩溃后重放到 safeOffset
- **批次**：`beginBatch` / `commitBatch` / `abortBatch`；支持 durable commit
- **事务 ID 幂等**：重复 commit 自动跳过；`txids.json` 记忆历史 ID
- **读快照一致性**：
  - `withSnapshot(fn)`：手动快照
  - QueryBuilder 在链路执行期间自动 epoch-pin
- **写锁**：`enableLock` 实现进程级独占写
- **读者注册**：`registerReader: true` 默认开启；维护任务尊重读者，避免破坏活跃快照
- **崩溃恢复流程**：
  1. 打开数据库
  2. 读取 WAL 并重放
  3. 合并幂等事务
  4. 如 manifest 异常，执行 `synapsedb check --strict`

更多细节见 `docs/教学文档/教程-04-事务-WAL-幂等.md`。

## 索引体系与性能调优

### 六序索引

- SPO / SOP / POS / PSO / OSP / OPS
- 基于前缀优先选择最佳索引
- 页大小可配置（默认 1024 左右）

### 属性索引

- 支持 `=`、`IN`、范围、前缀、模糊匹配
- 属性写入即刻可见（追加写 + 倒排文件）

### 热度驱动压实

- `synapsedb auto-compact`：基于热度、页数量、墓碑比例决定增量/整序策略
- 支持 `--include-lsm-segments` 自动并入 LSM 暂存

### 性能建议

- 热主键：优先 incremental 模式
- 冷数据：rewrite + Brotli 压缩
- WAL 过大：定期 `db.flush()` / `auto-compact` / `gc`
- 属性索引：合理规划字符串/数字类型，避免过大 JSON
- 更多调优技巧见 `docs/使用示例/性能基准测试指南.md`
- 快速回归检测：`pnpm bench:baseline`（默认 dry-run，无需额外参数）

## 运维与治理工具

| 类别     | 命令                                           | 典型场景                               |
| -------- | ---------------------------------------------- | -------------------------------------- |
| 诊断     | `synapsedb stats <db>`                         | 综合统计、热度、事务 ID、页分布        |
| 校验     | `synapsedb check <db> [--strict]`              | 索引/manifest/WAL 一致性校验           |
| 修复     | `synapsedb repair <db> [--fast]`               | 页映射修复或重建索引                   |
| 压实     | `synapsedb compact <db>`                       | 手动指定 orders/pageSize/压缩策略      |
| 自动治理 | `synapsedb auto-compact <db>`                  | 基于热度/墓碑阈值，增量压实并可自动 GC |
| 垃圾回收 | `synapsedb gc <db> [--respect-readers]`        | 清理孤页与无引用文件                   |
| 热点分析 | `synapsedb hot <db>`                           | 观察多页 primary / 热度排行            |
| 事务观测 | `synapsedb txids <db>`                         | 查询/清理事务 ID 注册表                |
| 导出调试 | `synapsedb dump <db> <order> <primary>`        | 查看指定主键所在页内容                 |
| 页级修复 | `synapsedb repair-page <db> <order> <primary>` | 对单个 primary 重建索引页              |

所有命令在 `pnpm db:*` 下有等价脚本。

推荐运维流程：

1. 日常运行：`synapsedb stats` + `synapsedb hot`
2. 周期治理：`synapsedb auto-compact` + `synapsedb gc`
3. 故障处理：`synapsedb check --strict` → `synapsedb repair`
4. 事务监控：`synapsedb stats --txids` / `synapsedb txids`

## 监控诊断与可观察性

- **日志**：CLI 默认输出 JSON-Like 日志，可重定向到文件
- **临时目录**：`tempfs.ts` 帮助在测试中验证持久化行为
- **统计采集**：`synapsedb stats` 支持 `--txids-window` 观察事务速率
- **热度与墓碑**：`hotness.json` / `index-manifest.json` 内含详细数据，可辅助二次分析
- **Benchmark 报表**：`benchmarks/*` 输出 TPS、延迟、内存、页分布
- **故障注入**：`src/utils/fault.ts` 支持模拟异常流程

## 集成模式与实践

- **嵌入后端服务**：直接在 Node.js 服务中 `open()`，结合 GraphQL/REST 层封装查询
- **CLI 自动化**：在 CI/CD 或定时任务中执行 `synapsedb auto-compact`、`synapsedb stats --summary`
- **MCP/LLM 生态**：配合 `repomix` 打包代码，作为 AI 助手的知识载体
- **VSCode 扩展**：可与自定义命令组合，实现一键导出/分析
- **数据导入/导出**：使用 `scripts/migrate-ndjson.mjs`、`benchmarks/*` 构建专用管道

## 示例与学习资源

- **官方教学**：`docs/教学文档/`（教程 00~09 + 实战案例）
- **操作示例**：`docs/使用示例/`（CLI、查询、事务、流式、图算法、迁移指南等）
- **知识库**：`.qoder/repowiki/zh/content/` 分专题深入解释
- **路线图**：`docs/项目发展路线图/` 与 `docs/milestones/`
- **测试分层**：`docs/测试分层与运行指南.md`
- **API/CLI 附录**：`docs/教学文档/附录-API参考.md`、`docs/教学文档/附录-CLI参考.md`

## 安全备份与合规

- 启用 `enableLock` 避免多进程并发写
- 备份策略：复制 `<db>.synapsedb` + `<db>.synapsedb.pages/` + `<db>.synapsedb.wal`
- 建议在备份前执行 `db.flush()` 或 `synapsedb auto-compact --auto-gc`
- 事务 ID 注册表可在恢复后清理：`synapsedb txids <db> --clear`
- CLI 默认启用路径校验，避免误写系统目录
- 不要将真实凭据写入属性；使用外部秘钥管理

## 路线图与版本策略

- 版本遵循 SemVer：`MAJOR.MINOR.PATCH`
- 每个里程碑详见 `docs/milestones/` 与 `CHANGELOG.md`
- 近期方向：
  - v1.2.x：查询增强（模式匹配 Planner 优化、多跳聚合）
  - v1.3.x：标准兼容（更多 Cypher/GraphQL 语法、Gremlin Pipeline）
  - v1.4.x：高级特性（分布式同步、增量备份、统计指标下沉）

## 常见问题 FAQ

1. **为何读取不到刚写入的数据？**
   - 写入后未 `flush()` 或未在同一快照内查询。调用 `db.flush()` 或使用 `withSnapshot(fn)`。
2. **WAL 文件持续增大怎么办？**
   - 定期 `db.flush()`；执行 `synapsedb auto-compact` + `synapsedb gc` 清理。
3. **多节点部署如何避免冲突？**
   - 每个实例配置独立路径；需要共享只读时，保持 `enableLock: false`。
4. **属性索引查询为什么慢？**
   - 检查属性是否为结构化 JSON；合理设置范围查询；避免过大数组。
5. **读取时看到历史数据？**
   - QueryBuilder 默认使用最新 manifest；若启用快照并长时间执行，可能仍在旧 epoch 中。
6. **GraphQL/Gremlin 查询抛错？**
   - 检查 schema 及查询语法是否符合支持范围；参考 `docs/使用示例` 中语法说明。
7. **如何导出全库？**
   - `synapsedb dump` + `scripts/dump-graph.mjs`；或使用 `repomix` 打包配合说明。
8. **如何接入现有系统？**
   - 参阅 `docs/使用示例/迁移指南-从Neo4j与TinkerGraph.md`，提供字段映射与脚本模板。

## 参与贡献与支持

- 贡献流程：
  1. Fork 仓库
  2. 建立 `feat/*` / `fix/*` 分支
  3. 修改并通过 `pnpm typecheck && pnpm lint && pnpm test && pnpm build`
  4. 遵循 Conventional Commits 撰写提交信息
  5. PR 需附验证方式与影响面说明
- Issue 与讨论：欢迎在 GitHub 提交问题、建议或分享实践
- 知识库维护：新增专题请同步更新 `.qoder/repowiki/zh/meta/repowiki-metadata.json`
- 欢迎贡献示例数据、benchmark 报告或教程

## 许可证

- **ISC License**
- 提交代码前请确认不包含真实凭据、私钥或生产数据
- 任何基于 SynapseDB 的分发请保留原许可证说明

---

> 更多细节请参考 `docs/教学文档` 与 `docs/使用示例` 系列文档。保持 `pnpm test` 与 `repomix` 结果为绿色，是所有改动的基本准线。
