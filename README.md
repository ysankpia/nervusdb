# SynapseDB（原型）

一个 TypeScript 实现的嵌入式“三元组（SPO）知识库”，支持分页索引、WAL v2 崩溃恢复、链式联想查询、Auto-Compact/GC 运维工具与读快照一致性。当前处于原型/Alpha 阶段。

## 快速开始

```ts
import { SynapseDB } from '@/synapseDb';

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

## CLI 运维

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

## 状态

- 存储/索引/WAL/查询/维护 已打通；P1 完成读快照一致性；P2 提供幂等事务 ID 原型与可选的跨周期去重。
- 更多细节参阅 `docs/SynapseDB设计文档.md`。
