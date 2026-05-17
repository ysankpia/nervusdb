# T49: v2 Crash Gate（证明你真的不会丢数据）

## 1. Context

v1 已经有 crash-gate 思路。v2 引入 manifest/segments/compaction 后，崩溃一致性复杂度更高，必须有可自动化的 crash 测试门禁。

## 2. Goals

- 提供一个可重复、可在 CI 低频跑的 crash harness
- 验证不变量（见 T45）：
  - committed 可见
  - 未 committed 不可见
  - manifest/segment 不悬挂

## 3. Harness 设计

- binary：`nervusdb-crash-test`（未来 crate 或 v2-storage 的 bin）
- 三段式：
  1. `driver`：循环 N 次启动子进程（writer/verify）
  2. `writer`：执行随机事务序列（create_node/create_edge/tombstone/compact）
  3. `verify`：重启后检查图一致性（IDMap 可解析、neighbors 不违反 tombstone、manifest epoch 单调）

## 4. CI 策略

- PR：跑小次数（例如 30-100 次）
- nightly/release：跑大次数（1000+）

