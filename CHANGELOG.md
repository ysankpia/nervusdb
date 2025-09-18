# 变更日志

## v0.2.0

- P0：WAL v2 合流与清理、尾部安全截断测试、写锁/读者参数对齐、基础 CI 接入
- P1：读快照一致性 `withSnapshot(fn)`、QueryBuilder 链路固定 epoch、运维组合用例与文档补充
- P2（阶段一）：
  - 事务批次原型增强：`beginBatch({ txId?, sessionId? })`，WAL `BEGIN` 携带元信息
  - 重放幂等：相同 `txId` 的重复 COMMIT 跳过；属性与 abort 语义测试覆盖
  - 持久化去重（可选）：`enablePersistentTxDedupe` + `maxRememberTxIds` + `txids.json`
  - CLI：`db:txids`、`db:stats --txids[=N]`；README 与设计文档新增说明

> 重要：旧 WAL 可兼容重放；若出现历史不一致，建议先执行 `pnpm db:repair` 与 `pnpm db:check --strict`。

