# SynapseDB（原型）

一个用 TypeScript/Node.js 实现的嵌入式“三元组（SPO）知识库”。面向代码知识、配置/关系图谱、轻量推理与链式联想的本地/边缘嵌入式场景，强调“可恢复、可治理、可扩展”。支持分页索引、WAL v2 崩溃恢复、链式联想查询、Auto‑Compact/GC 运维工具与读快照一致性。当前处于原型/Alpha 阶段。

核心特性（Highlights）
- 单文件主数据 + 分页索引：`*.synapsedb` + `*.synapsedb.pages/`
- 六序索引（SPO/SOP/POS/PSO/OSP/OPS）与增量分页合并（Compaction incremental/rewrite）
- WAL v2：批次 `BEGIN/COMMIT/ABORT`，崩溃后可重放，尾部安全截断
- 读快照一致性：查询链路 epoch-pin，期间 manifest 固定不漂移
- 热度统计与半衰：`hotness.json` 记录 primary 热度，支持热度驱动合并
- 读者注册：`readers.json`（跨进程）用于尊重读者的运维
- LSM‑Lite 暂存（实验）：旁路段 `lsm-manifest.json` 可并入索引
- 事务幂等（实验）：可选 `txId/sessionId`，支持跨周期幂等去重
- 进程级写锁（可选）：`enableLock` 保证同一路径独占写
- CLI 全覆盖：检查/修复/治理/导出/热点/事务观测 一条龙

## 快速开始（作为库）

```ts
// 生产/项目使用（ESM）：
import { SynapseDB } from 'synapsedb';

const db = await SynapseDB.open('brain.synapsedb');

db.addFact({ subject: 'file:/src/user.ts', predicate: 'DEFINES', object: 'class:User' });
db.addFact({ subject: 'class:User', predicate: 'HAS_METHOD', object: 'method:login' });

const authors = await db.withSnapshot(async (snap) => {
  return snap
    .find({ object: 'method:login' })
    .followReverse('HAS_METHOD')
    .followReverse('DEFINES')
    .all();
});

await db.flush();
```

- 读快照一致性：`withSnapshot(fn)` 在回调内固定 manifest `epoch`，避免后台 compaction/GC 导致视图漂移。
- 链式查询：`find().follow()/followReverse().where().limit().anchor()`，执行期间自动 pin/unpin epoch。

环境与模块说明：
- 运行时要求 Node.js 18+（推荐 20+）。
- 包为 ESM（`package.json: { "type": "module" }`），请确保你的项目也是 ESM 环境或使用支持 ESM 的打包器。
- 若在本仓库内开发，可继续使用 `@` → `src/` 的路径别名（见 `vitest.config.ts`）。

## 安装与全局 CLI

本地全局安装（开发者从源码安装）：

```bash
# 在仓库根目录
pnpm build    # 或 npm run build
npm i -g .    # 将当前包全局安装（生成 synapsedb 命令）
```

安装完成后可使用 `synapsedb` 命令：

```bash
synapsedb --help
synapsedb bench demo.synapsedb 100 lsm
synapsedb stats demo.synapsedb
```

CLI 子命令速览（语义与 `pnpm db:*` 脚本等价）：

- `synapsedb check <db> [--summary|--strict]`
- `synapsedb repair <db> [--fast]`
- `synapsedb compact <db> [--orders=SPO,POS] [--page-size=1024] [--min-merge=2] [--tombstone-threshold=0.2] [--mode=rewrite|incremental] [--dry-run] [--compression=brotli:4|none]`
- `synapsedb auto-compact <db> [--mode=incremental] [--orders=...] [--min-merge=2] [--hot-threshold=H] [--max-primary=K] [--auto-gc]`
- `synapsedb gc <db> [--no-respect-readers]`
- `synapsedb stats <db> [--txids[=N]] [--txids-window=MIN]`
- `synapsedb txids <db> [--list[=N]] [--since=MIN] [--session=ID] [--max=N] [--clear]`
- `synapsedb dump <db> <order:SPO|SOP|POS|PSO|OSP|OPS> <primary:number>`
- `synapsedb hot <db> [--order=SPO] [--top=10]`
- `synapsedb repair-page <db> <order> <primary>`

示例输出说明（以 `synapsedb stats` 为例）：
- `dictionaryEntries`：字典条目数（字符串→ID）
- `triples`：主数据三元组条数（不含 tombstones）
- `epoch`：manifest 版本（每次合并/更新递增）
- `pageFiles`/`pages`：索引页文件数量/总页数
- `tombstones`：逻辑删除计数
- `walBytes`：WAL 文件大小（字节）
- `txIds`：持久化事务 ID 条数（启用幂等后可见）
- `orders.*.multiPagePrimaries`：拥有多页的 primary 数（合并候选）

## 事务批次与幂等（实验性）

为应对“至少一次投递”的失败重试场景，支持可选的 txId/会话标识：

```ts
const db = await SynapseDB.open('tx.synapsedb', {
  enablePersistentTxDedupe: true, // 开启跨周期幂等去重（可选）
  maxRememberTxIds: 2000,         // 最多记忆最近 2000 个 txId（可选）
});

db.beginBatch({ txId: 'T-123', sessionId: 'writer-A' });
db.addFact({ subject: 'A', predicate: 'R', object: 'X' });
db.commitBatch();
```

- 单次重放幂等：WAL 重放时，同一 `txId` 的重复 COMMIT 将被跳过。
- 跨周期幂等：开启 `enablePersistentTxDedupe` 后，重放会读取 `<db>.synapsedb.pages/txids.json` 中的历史 txId，跳过重复提交；commit 成功后会异步写入 txId。
- 边界：注册表仅用于崩溃恢复场景的重放去重；并不改变实时写入的覆盖语义。

### 失败重试最佳实践

- 为每次重试使用相同的 `txId`，确保重放/恢复时为“至多一次”效果；避免在同一逻辑事务内混用不同 `txId`。
- 对属性写入（覆盖语义）尤其推荐使用 `txId`，防止因重复重放导致的最后写入值异常。
- 建议为写入流量分配 `sessionId`（例如实例 ID），方便在日志/观测中定位问题来源。
- 注册表有容量上限（`maxRememberTxIds`）；应结合业务的重试窗口合理配置，防止过早遗忘导致重复生效。

## API 概览（库用）

类型与入口：`import { SynapseDB } from 'synapsedb'`

- `SynapseDB.open(path, options?)`：打开/创建数据库
- `addFact({ subject, predicate, object }, { subjectProperties?, objectProperties?, edgeProperties? })`
- `find(criteria, { anchor? })` → `QueryBuilder`
  - `follow(predicate)` / `followReverse(predicate)` / `where(fn)` / `limit(n)` / `all()`
- 流式查询：`for await (const batch of db.streamFacts({ predicate: 'R' }, 1000)) { ... }`
- 属性：`getNodeProperties(nodeId)` / `getEdgeProperties({subjectId,predicateId,objectId})`
- 列表：`listFacts()`；删除：`deleteFact({ s,p,o })`
- 批次：`beginBatch({ txId?, sessionId? })` / `commitBatch({ durable? })` / `abortBatch()`
- 刷盘：`flush()`（持久化数据/索引、重置 WAL、写 hotness）
- 关闭：`close()`（释放写锁、注销读者）

打开参数（`SynapseDBOpenOptions` 要点）
- `indexDirectory`：索引目录（默认 `path + '.pages'`）
- `pageSize`：每页三元组数（默认 1000～1024 量级）
- `rebuildIndexes`：强制在下次 open 时重建分页索引
- `compression`：`{ codec: 'none' | 'brotli', level?: 1~11 }`
- `enableLock`：启用进程级独占写锁；生产建议开启
- `registerReader`：是否登记为读者（默认 true），运维工具会尊重
- `stagingMode`：`'default' | 'lsm-lite'`（实验）
- `enablePersistentTxDedupe`：启用跨周期 txId 幂等
- `maxRememberTxIds`：记忆 txId 上限（默认 1000）

查询模型与索引选择
- 条件为 `subject/predicate/object` 任意组合；内部按覆盖前缀选取最佳顺序（如 `s+p` → `SPO`）
- `anchor: 'subject' | 'object' | 'both'` 决定联想查询初始前沿
- 读快照一致性：`withSnapshot(fn)` 在回调内固定 manifest；`QueryBuilder` 链式期间同样固定

属性与版本
- 节点/边属性以 JSON 序列化存储，边属性键为 `subjectId:predicateId:objectId`
- 多次覆盖写会升级 `__v`（版本号）

删除与 tombstones
- `deleteFact({ s,p,o })` 仅写入 tombstone，查询自动过滤；合并/GC 后由 manifest 与页面级 GC 清理无引用页

## 运维与治理（CLI/脚本）

仓库内也可通过 PNPM 脚本使用（等价功能）：

- 统计：`pnpm db:stats <db>`（输出 `triples/epoch/pages/tombstones/walBytes/txIds`）
- 自动合并：`pnpm db:auto-compact <db> [--mode=incremental] [--orders=...] [--hot-threshold=H] [--auto-gc]`
- GC：`pnpm db:gc <db>`
- 修复/检查/导出：`pnpm db:repair` / `pnpm db:check` / `pnpm db:dump`
- 热点：`pnpm db:hot <db>`
- txId 观测：
  - `pnpm db:stats <db> --txids[=N]`：展示最近 N 条 txId（默认 50）
  - `pnpm db:stats <db> --txids-window=MIN`：统计最近 MIN 分钟内 txId 数量与按 session 聚合
  - `pnpm db:txids <db> [--list[=N]] [--since=MIN] [--session=ID] [--max=N] [--clear]`
    - `--list[=N]`：按时间倒序列出最近 N（默认 50）
    - `--since=MIN`：仅显示最近 MIN 分钟内的条目
    - `--session=ID`：仅显示指定 sessionId 的条目
    - `--max=N`：设置/裁剪注册表容量上限
    - `--clear`：清空注册表（谨慎使用）

Compaction（合并）策略
- rewrite：全量重写指定顺序的页文件（压缩比高，I/O 较大）
- incremental：仅为目标 primary 追加新页并替换映射（更快，适用于热主键/多页场景）
- 选择标准：`min-merge`（多页阈值）/ `tombstone-threshold`（墓碑比例）/ 热度驱动（`hot-threshold` + TopK）
- LSM 段并入：`--includeLsmSegments` 或 `--includeLsmSegmentsAuto`（满足阈值时自动并入并清空段）

GC（页面级）
- 针对增量重写后遗留的 `orphans`（孤页）进行目录内文件收缩
- 建议在有读者时启用 `--respect-readers` 保障查询安全

Repair（修复）
- `repair --fast`：按页（primary）快速修复，仅替换坏页映射
- 未发现坏页则尝试“按序重写”；仍无则全量重建（保留 tombstones）

## 示例：从 0 到可用

```bash
# 生成一个测试库（LSM-Lite 暂存演示），并查看统计
synapsedb bench repo_demo.synapsedb 100 lsm
synapsedb stats repo_demo.synapsedb

# 执行一次增量合并（仅对 SPO 顺序，阈值=2，热度阈值=1，仅 Top1，合并后自动 GC）
synapsedb auto-compact repo_demo.synapsedb \
  --mode=incremental --orders=SPO --min-merge=2 --hot-threshold=1 --max-primary=1 --auto-gc

# 导出某个主键下的页（调试/排查）
synapsedb dump repo_demo.synapsedb SPO 1
```

清理临时样本库（可选）：

```bash
rm -rf repo_demo.synapsedb repo_demo.synapsedb.pages repo_demo.synapsedb.wal
```

## 状态

- 存储/索引/WAL/查询/维护 已打通；P1 完成读快照一致性；P2 提供幂等事务 ID 原型与可选的跨周期去重。
- 更多细节参阅 `docs/SynapseDB设计文档.md`。

## 文档目录（使用教程）

- 教程-00-概览：docs/教学文档/教程-00-概览.md
- 教程-01-安装与环境：docs/教学文档/教程-01-安装与环境.md
- 教程-02-数据模型与基础 CRUD：docs/教学文档/教程-02-数据模型与基础CRUD.md
- 教程-03-查询与链式联想：docs/教学文档/教程-03-查询与链式联想.md
- 教程-04-事务、WAL 与幂等：docs/教学文档/教程-04-事务-WAL-幂等.md
- 教程-05-索引选择与性能：docs/教学文档/教程-05-索引选择与性能.md
- 教程-06-维护与治理：docs/教学文档/教程-06-维护与治理.md
- 教程-07-存储格式与持久化：docs/教学文档/教程-07-存储格式与持久化.md
- 教程-08-部署与最佳实践：docs/教学文档/教程-08-部署与最佳实践.md
- 教程-09-FAQ 与排错：docs/教学文档/教程-09-FAQ与排错.md
- 附录-CLI 参考：docs/教学文档/附录-CLI参考.md
- 附录-API 参考：docs/教学文档/附录-API参考.md
  
## 实战案例

- 实战-代码知识图谱：docs/教学文档/实战-代码知识图谱.md
- 实战-商城系统：docs/教学文档/实战-商城系统.md

## 架构与存储布局（概览）

- 主数据：`<name>.synapsedb`
  - 64B 文件头（魔数 `SYNAPSEDB`，版本 `2`），区段：`dictionary/triples/indexes(staging)/properties`
- 分页索引：`<name>.synapsedb.pages/`
  - `*.idxpage`：按顺序持久化的页文件（压缩可选 brotli）
  - `index-manifest.json`：包含 `pageSize/compression/lookups/tombstones/epoch/orphans`
  - 元数据：`hotness.json`（热度计数，半衰）、`readers.json`（读者注册，跨进程可见）
  - 旁路：`lsm-manifest.json`（段清单，支持并入与清空）
- 写入日志：`<name>.synapsedb.wal`（WAL v2，崩溃可重放并在校验失败处安全截断）

一致性与恢复
- WAL 重放顺序：add/delete/props → safeOffset 截断 → 合并 txId 注册表（如启用）
- Manifest 原子更新：`*.tmp` 写入与目录 fsync，崩溃不留临时文件
- 查询快照：epoch-pin，链式联想期间 manifest 不变

## 性能与调优建议

- `pageSize`：小页面减少一次读取成本但增加页数；建议 1K~2K 之间按场景评估
- 压缩：`brotli` 在冷数据上很有价值；增量重写时热主键可选择更低级别或 `none`
- 合并模式：日常以 incremental 为主，定期 rewrite 清理与提升压缩比
- 热度阈值：结合业务访问特征设置 `hot-threshold`，TopK 限制 `max-primary`
- 生产运行：
  - 强烈建议 `enableLock: true`
  - 保持 `registerReader: true`，运维工具采用尊重读者策略
  - 治理任务先 `--dry-run` 获取统计，再执行；治理后可 `gc` 清理 orphans
  - 关键事务使用 `commit({ durable: true })` 获取更强持久性保证

## 常见问题（FAQ）

- ERR_MODULE_NOT_FOUND（ESM 导入失败）
  - 请确保 Node 18+ 且工程为 ESM（`"type":"module"`）；CommonJS 中可使用 `await import('synapsedb')`。
- 全局安装后找不到命令
  - 优先使用 `npm i -g .`（自动在 PATH 中创建 bin）；使用 pnpm 需先 `pnpm setup` 配置 PNPM_HOME。
- manifest 缺失或索引损坏
  - 使用 `synapsedb check <db> --strict` 定位问题，`synapsedb repair <db> [--fast]` 修复；必要时加 `--rebuildIndexes` 重新打开。
- 并发写导致冲突
  - 生产请开启 `enableLock`；读者不受写锁限制，建议读者登记开启。

## 开发与测试

- 安装依赖：`pnpm install`
- 类型检查：`pnpm typecheck`
- 代码规范：`pnpm lint` / `pnpm lint:fix`
- 单元测试：`pnpm test` / `pnpm test:coverage`
- 构建发布：`pnpm build`；打包分发：`pnpm pack`
