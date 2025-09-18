# 教程 04 · 事务、WAL 与幂等

## 批次事务（内存级）

```ts
db.beginBatch({ txId: 'T-001', sessionId: 'writer-A' });
db.addFact({ subject: 'A', predicate: 'R', object: 'X' });
db.addFact({ subject: 'A', predicate: 'R', object: 'Y' });
db.commitBatch({ durable: true }); // 可选：持久化落盘（fsync）
```

- `durable: true`：更强持久性保障，成本是提交延迟更高
- `abortBatch()`：放弃本批次修改

## WAL v2 崩溃恢复

- 写入先进入 WAL，崩溃后通过 WalReplayer 重放恢复
- 尾部安全截断：遇到不完整记录仅保留 `safeOffset`

## 幂等（实验）

为“至少一次投递”的失败重试场景提供可选的跨周期幂等：

```ts
const db = await SynapseDB.open('tx.synapsedb', {
  enablePersistentTxDedupe: true,
  maxRememberTxIds: 2000,
});

db.beginBatch({ txId: 'ORD-2025-00001', sessionId: 'svc-A' });
// ... 写入
db.commitBatch();
```

- 重放时同一 `txId` 的重复提交将被跳过
- `*.synapsedb.pages/txids.json` 记录历史 txId（受上限配置）

## CLI 观测与治理

- `synapsedb stats <db> --txids[=N] --txids-window=MIN`：查看事务时间窗与会话聚合
- `synapsedb txids <db> [--list[=N]] [--since=MIN] [--session=ID] [--max=N] [--clear]`
