# Repository Guidelines（项目协作与实现对齐）

本指南面向在本仓库内协作的开发者与智能体，确保对项目结构、对外 API、构建测试流程、存储格式与查询模型的认知与实际实现保持一致。所有文档与代码注释请使用中文。

## 项目总览
- 技术栈：TypeScript + Node.js（建议 Node 18+）。
- 模型定位：SPO（三元组）原生的轻量嵌入式“类人脑”知识库，支持链式联想与属性存储。
- 产物形态：单文件存储（`.synapsedb`）+ 分页索引目录（`*.synapsedb.pages/`）+ 增量 WAL（`*.synapsedb.wal`）。
- 已实现能力：WAL v2 事务批次、六序索引与分页化磁盘索引（可 Brotli 压缩）、增量/整序 compaction、页面级 GC、热度统计与半衰衰减、读一致性（epoch-pin）、读者注册与尊重读者的治理流程。

## 目录结构与关键文件
- 源码根目录：`src/`
  - 入口与聚合导出：`src/index.ts`（连接 URI 构建工具、类型导出、顶层 API 聚合）
  - 对外数据库 API：`src/synapseDb.ts`（`SynapseDB.open/addFact/find/follow/...`）
  - 联想查询：`src/query/queryBuilder.ts`（`find/follow/followReverse/all` 与锚点 `anchor`）
  - 存储子系统：`src/storage/`
    - 字典：`dictionary.ts`（字符串 ↔ ID）
    - 三元组：`tripleStore.ts`（SPO 编码）
    - 暂存索引：`tripleIndexes.ts`（六序索引增量分桶、序列化）
    - 属性：`propertyStore.ts`（节点/边属性 JSON 序列化，含版本自增 `__v`）
    - 文件头/布局：`fileHeader.ts`、`layout.ts`（魔数 `SYNAPSEDB`，版本 2，64B 头）
  - 分页索引：`pagedIndex.ts`（`.idxpage` 分页文件、`index-manifest.json`、支持 `brotli` 压缩，含 `epoch/tombstones/orphans`）
  - 热度与读者：`hotness.ts`（`hotness.json`），`readerRegistry.ts`（`readers.json`）
  - 写入日志：`wal.ts`（WAL v2，记录 add/delete/props 与批次提交，支持重放与尾部截断）
- 设计文档：`docs/SynapseDB设计文档.md`
- 测试：`tests/`（Vitest 覆盖持久化、WAL 恢复、索引选择、联想查询等）
- 构建输出：`dist/`
- 别名：在 `vitest.config.ts` 中配置 `@` → `src/`

## 对外 API 清单（`src/index.ts` 与 `src/synapseDb.ts`）
- 连接工具（面向外部系统连接字符串）：
  - `ensureConnectionOptions(options)`：校验并补全端口
  - `buildConnectionUri(options)`：生成稳定的连接 URI
  - `sanitizeConnectionOptions(options)`：口令仅保留末四位，其余打码
- 嵌入式数据库：
  - `class SynapseDB`：
    - `open(path, { indexDirectory?, pageSize?, rebuildIndexes?, compression?, enableLock?, registerReader? })`
    - `addFact(fact, { subjectProperties?, objectProperties?, edgeProperties? })`
    - `find(criteria, { anchor? })` → `QueryBuilder`
    - `deleteFact(fact)`、`listFacts()`、`flush()`
    - 事务批次：`beginBatch()`、`commitBatch()`、`abortBatch()`（可选）
    - 生命周期：`close()`（释放写锁、取消读者登记）
    - `getNodeId(value)`、`getNodeValue(id)`、`getNodeProperties(id)`、`getEdgeProperties(key)`
  - `class QueryBuilder`：`follow(predicate)`、`followReverse(predicate)`、`all()`
- 导出类型：`FactInput`、`PersistedFact`、`FactRecord`、`FactCriteria`、`FrontierOrientation`

## 存储格式与持久化
- 主数据文件（单文件）：`<name>.synapsedb`
  - 文件头：魔数 `SYNAPSEDB`、版本 `2`、长度 `64` 字节
  - 区段：`dictionary`、`triples`、`indexes(staging)`、`properties`
- 分页索引目录：`<name>.synapsedb.pages/`
  - 页文件：`SPO.idxpage`、`SOP.idxpage`、`POS.idxpage`、`PSO.idxpage`、`OSP.idxpage`、`OPS.idxpage`
  - 清单：`index-manifest.json`（`pageSize/compression/lookups/tombstones/epoch/orphans`）
  - 周边元数据：`hotness.json`（热点计数，带半衰衰减）、`readers.json`（活动读者登记）
  - 压缩：支持 `{ codec: 'none' | 'brotli', level?: 1~11 }`
- 增量日志：`<name>.synapsedb.wal`（WAL v2，支持批次 `BEGIN/COMMIT/ABORT`；追加写，崩溃后由 `WalReplayer` 重放并在校验失败处进行尾部安全截断）
- 刷新：调用 `db.flush()` 将字典/三元组/属性持久化，写入或增量合并分页索引，落盘并重置 WAL

## 查询模型与索引
- 查询入口：`SynapseDB.find(criteria, { anchor? })`，criteria 为 `subject/predicate/object` 的任意组合
- 锚点 `anchor`：`'subject' | 'object' | 'both'`，决定初始前沿（frontier）侧重
- 六序增量索引选择策略（`getBestIndexKey`）：优先覆盖更多前缀（如 `s+p` → `SPO`）
- 链式联想：`follow`（正向，主语→宾语）与 `followReverse`（反向，宾语→主语）
- 结果下推：`where(fn)` 在页读后进行最小过滤；`limit(n)` 限制结果集大小；`anchor('subject'|'object'|'both')` 重新锚定前沿便于继续联想
- 读一致性：链式查询期间固定 manifest `epoch`（epoch-pin），避免 compaction/GC 中途重载影响同一条查询链路
- 去重与前沿推进：同跳以 `subjectId:predicateId:objectId` 键去重并推进下一跳 frontier

## 构建、测试与开发命令（`package.json`）
- 安装依赖：`pnpm install`
- 开发监听：`pnpm dev`（`tsx watch src/index.ts`）
- 构建编译：`pnpm build`（输出到 `dist/`）
- 类型检查：`pnpm typecheck`
- 代码规范：`pnpm lint` / `pnpm lint:fix`
- 单测运行：`pnpm test` / `pnpm test:watch`
- 覆盖率：`pnpm test:coverage`（V8，报告位于 `coverage/`）
- 维护工具：
  - `pnpm db:check [--summary|--strict]`（概览/严格校验，概览含 `epoch`、每序页统计、`orphans` 数）
  - `pnpm db:repair [--fast]`（优先按页快速修复；无损坏→按序→全量重建）/ `pnpm db:repair-page <db> <order> <primary>`
  - `pnpm db:compact [--mode incremental|rewrite] [--orders=SPO,POS] [--min-merge=N] [--tombstone-threshold=R] [--only-primaries=SPO:1,2;POS:3] [--compression=brotli:4|none] [--dry-run]`
  - `pnpm db:auto-compact [--mode=incremental] [--orders=...] [--min-merge=N] [--hot-threshold=H] [--max-primary=K] [--respect-readers] [--auto-gc] [--dry-run]`（多因素评分：`scoreWeights.hot/pages/tomb`、`minScore` 可在代码侧配置）
  - `pnpm db:gc [--respect-readers]`（尊重读者时有活动读者则跳过清理）
  - `pnpm db:hot`（热点主键 TopN） / `pnpm db:stats`（规模/顺序概览） / `pnpm db:dump`（页导出） / `pnpm bench`
  - 运行参数：`enableLock`（独占写锁）、`registerReader`（读者登记，跨进程可见）

## 测试规范与覆盖率（Vitest）
- 位置与命名：`tests/**/*.test.ts`
- 主题覆盖：
  - `persistentStore.test.ts`：首次持久化与属性读写
  - `wal.test.ts`：未 flush 写入的 WAL 重放恢复
  - `wal_v2.test.ts`：批次提交语义（BEGIN/COMMIT/ABORT）
  - `tripleIndexes.test.ts`：六序分桶、序列化/反序列化
  - `index_order_selection.test.ts`：索引选择策略
  - `queryBuilder.test.ts`：多跳联想、where/limit/anchor 与读一致性
  - `delete_update.test.ts`：逻辑删除 + 属性更新一致性
  - `compaction*.test.ts`：整序/增量/高级选项/评分驱动
  - `gc*.test.ts`：页面级 GC（含尊重读者）
  - `repair*.test.ts`：按页/按序修复
  - `crash_injection.test.ts`：flush 路径崩溃注入一致性
  - `lockfile.test.ts`：进程级写锁
- 质量门槛（见 `vitest.config.ts`）：Statements ≥80%，Branches ≥75%，Functions ≥80%，Lines ≥80%

## 编码风格与命名约定
- 统一格式：见 `.prettierrc`（单引号、分号、尾随逗号、宽度 100、缩进 2）
- Lint：ESLint flat config（`eslint.config.js`），启用 `@typescript-eslint` 与 Prettier 规则
- 文件命名：短横线风格；类型与常量使用清晰语义命名；模块导出以动词函数或名词化类型为主
- 路径别名：`@` 指向 `src/`（测试与源码均可使用）

## 提交与拉取请求规范
- 提交信息：遵循 Conventional Commits（如 `feat: 分页索引支持 brotli 压缩`）
- 提交流程：在提交前通过 `typecheck`、`lint`、`test`、`build`，PR 描述需列出影响面与验证方式
- 文档同步：涉及外部 API 或脚本变更需更新 `AGENTS.md`/示例/设计文档

## 安全与配置提示
- 不提交真实凭据或生产 URI；敏感信息仅通过环境变量传递
- 如新增外部依赖或服务接入，请在 PR 中说明鉴权策略、回滚思路与最小权限配置

## 使用示例（最小可运行）
```ts
import { SynapseDB } from '@/synapseDb';

const db = await SynapseDB.open('brain.synapsedb');
db.addFact({ subject: 'file:/src/user.ts', predicate: 'DEFINES', object: 'class:User' });
db.addFact({ subject: 'class:User', predicate: 'HAS_METHOD', object: 'method:login' });

const authors = db
  .find({ object: 'method:login' })
  .followReverse('HAS_METHOD')
  .followReverse('DEFINES')
  .all();

await db.flush();
```

## 常见注意事项
- 写入后请调用 `flush()` 以持久化并增量合并分页索引，同时重置 WAL
- `rebuildIndexes: true` 可在下次 `open()` 时强制重建分页索引（或 `pageSize` 变更时自动重建）
- 逻辑删除通过 tombstones 记录，重启后由 manifest 恢复；查询会自动过滤被删除的三元组
- 属性存储采用 JSON 序列化并维护版本号 `__v`；多次覆盖写入会提升版本
- 生产运行建议：auto-compact 与 GC 默认尊重活动读者；在批量治理前先 `--dry-run` 获取统计，并优先选择增量模式与 TopK 精确重写；治理后可运行 `db:gc --respect-readers` 清理 `orphans`
- 诊断：`db:check --summary` 关注 epoch、orphans 与多页 primary；热点用 `db:hot` 着重观察高热度 + 多页主键

（本文件将随实现演进持续更新，若发现与代码不一致，请以源码为准并提交修订）
