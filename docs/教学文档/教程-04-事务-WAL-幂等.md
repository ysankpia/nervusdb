# 教程 04 · 事务、WAL 与幂等

## 目标

- 掌握批次写入、事务 ID 与 WAL v2 的使用方式
- 了解崩溃恢复流程与幂等语义
- 学会使用 `withSnapshot`、`registerReader` 等读治理手段

## 前置要求

- 已熟悉基础 CRUD 与 QueryBuilder
- 本教程假设数据库路径为 `demo.nervusdb`

## 事务批次

```ts
const db = await NervusDB.open('demo.nervusdb', {
  enableLock: true,
  enablePersistentTxDedupe: true,
});

try {
  db.beginBatch({ txId: 'order-2025-001', sessionId: 'billing-service' });
  db.addFact({ subject: 'order:2025-001', predicate: 'BELONGS_TO', object: 'user:alice' });
  db.addFact({ subject: 'order:2025-001', predicate: 'CONTAINS', object: 'sku:keyboard' });
  db.commitBatch({ durable: true });
} catch (err) {
  db.abortBatch();
  throw err;
}
```

- `txId`：保证重复提交不生效
- `sessionId`：标识写入来源，便于观测
- `durable: true`：在 commit 时强制 fsync，加固持久性

## 幂等与重放

- WAL 重放时，若遇到相同 `txId` 的 `COMMIT`，会跳过写入（防止重复）
- `enablePersistentTxDedupe`：在 `<db>.nervusdb.pages/txids.json` 中记录历史 `txId`
- `maxRememberTxIds`：控制记忆容量（默认 1000）

## 崩溃恢复流程

1. 重新 `open()` 数据库
2. 扫描 WAL，重放事务、属性、删除操作
3. 如果 WAL 尾部校验失败，自动截断到安全位置
4. 合并 `txId`，跳过已执行事务
5. 载入 manifest、热度、读者信息

可通过以下指令验证：

```bash
nervusdb check demo.nervusdb --summary
nervusdb txids demo.nervusdb --list=20
```

## 读一致性

- `withSnapshot(fn)`：回调期间固定 manifest epoch，适合长链查询
- `registerReader: true`（默认）：登记读者，自动写入 `readers.json`
- `nervusdb auto-compact` 会在有活跃读者时跳过相关主键

## 多写者与锁

- 默认允许多进程并发写；开启 `enableLock` 实现单写者
- 读者不受写锁影响，可启用 `registerReader` 保障快照期间安全
- 判断写锁：`nervusdb stats --summary` 输出 `lock: true/false`

## WAL 文件结构

| 字段       | 说明                                             |
| ---------- | ------------------------------------------------ |
| header     | 魔数、版本、序列号                               |
| entry type | ADD / DELETE / PROPERTY / BEGIN / COMMIT / ABORT |
| payload    | 三元组或属性数据、事务元信息                     |
| checksum   | CRC 校验，保障重放安全                           |

详细结构见 `docs/教学文档/教程-07-存储格式与持久化.md`。

## 常见问题

| 情况               | 分析                      | 解决                                                 |
| ------------------ | ------------------------- | ---------------------------------------------------- |
| `commitBatch` 抛错 | 批次尚未开始、重复 commit | 确认 `beginBatch` 调用顺序；异常时及时 `abortBatch`  |
| WAL 持续变大       | 未 flush 或治理           | 定期 `db.flush()`、`nervusdb auto-compact`           |
| 重放时间长         | WAL 涉及大量历史事务      | 执行 `nervusdb auto-compact --mode=rewrite` 减少 WAL |
| 幂等失效           | 未设置 `txId`             | 关键事务必须提供稳定 `txId`                          |

## 验证练习

1. 模拟在批次中抛错并验证 `abortBatch` 行为
2. 手动修改 WAL（如新增无效 entry），使用 `nervusdb check --strict` 观察诊断结果
3. 使用 `nervusdb txids --clear` 清空注册表，再次重复提交验证幂等失效

## 延伸阅读

- [docs/使用示例/04-事务与幂等-示例.md](../使用示例/04-事务与幂等-示例.md)
- [docs/使用示例/07-快照一致性与并发-示例.md](../使用示例/07-快照一致性与并发-示例.md)
- [docs/使用示例/迁移指南-从Neo4j与TinkerGraph.md](../使用示例/迁移指南-从Neo4j与TinkerGraph.md)
