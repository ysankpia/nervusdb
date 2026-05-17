# T45: v2 Durability / Checkpoint / Crash Model（别自欺欺人）

## 1. Context

v2 采用 `.ndb + .wal`，WAL 是唯一耐久来源（delta 持久化形式）。只要 crash model 没写死，你就会在“什么时候 fsync / 什么时候 flush / manifest 怎么更新”里永远打补丁。

## 2. Goals

- 定义明确的 durability contract（默认 Full）
- 定义 checkpoint/manifest 的原子切换与崩溃恢复流程
- 保证恢复后满足不变量：
  - committed 事务可见
  - 未 committed 不可见
  - manifest 不出现悬挂引用

## 3. Durability Levels（对外 API）

- `Full`（默认）：
  - commit 时 `fsync(.wal)` 必须完成
  - `.ndb` 的 flush 可以延后（由 checkpoint/compaction 决定）
- `GroupCommit{max_delay_ms, max_bytes}`：
  - 多次 commit 合并一次 `fsync(.wal)`
- `None`：
  - 不 fsync（仅开发/bench 使用）

## 4. 写入顺序（必须固定）

### 4.1 普通写事务（M1）

1. append WAL records（BeginTx + ops + CommitTx）
2. `fsync(.wal)`（Full）
3. apply 到内存（MemTable → freeze L0Run → publish）
4. `.ndb` 的 I2E 更新可以是：
   - A) 仅靠 replay 重建（更简单，代价：启动更慢）
   - B) commit 后写 `.ndb`（更快启动，要求 flush 顺序更严格）

**M1 推荐**：先走 A（简化），M2 之后再引入 B。

## 5. Manifest（段集合的真相来源）

M2 引入 CSR segments 后，必须有一个“段列表”的耐久来源（manifest），否则 crash 后无法知道哪些 segment 有效。

### 5.1 Manifest 存放位置

- 存在 `.ndb` 的固定位置（例如 meta page 指向一棵 append-only manifest log）
- 或者存在 `.wal`（以 special record 表示“段集合切换”）

**建议**：manifest 走 WAL（更符合“WAL 是唯一耐久真相”），并在 checkpoint 时写回 `.ndb`。

### 5.2 原子切换规则

生成新 segment 时：

1. 写入 segment pages（`.ndb`）
2. `sync_data(.ndb)`（确保 segment 内容落盘）
3. append `ManifestSwitch{new_manifest_epoch, segments...}` 到 `.wal`
4. `fsync(.wal)`（Full）

恢复时以 WAL 中最后一个完整的 `ManifestSwitch` 作为可见段集合（last writer wins）。

## 6. Checkpoint（什么时候可以截断 WAL）

**定义**：当且仅当 `.ndb` 已经包含了恢复所需的全部状态（包含 segment + manifest + IDMap 等），才允许截断 WAL。

MVP（先简单）：

- 不截断 WAL，只追加（让系统先正确）
- 后续再加 checkpoint：`db.checkpoint()` 显式触发

## 7. Recovery Algorithm（启动）

1. open `.ndb`
2. replay `.wal`：
   - 跳过尾部不完整 record
   - 只应用 committed tx
   - 读最后一个 `ManifestSwitch`（如存在）
3. 构建 runtime state：
   - published segments（来自 manifest）
   - published runs（来自 committed graph tx 冻结）
   - IDMap：从 `.ndb` I2E 扫描（或 replay 重建）

## 8. Invariants（测试必须验证）

- 不存在“已在 manifest 引用但 segment 不完整”的状态
- 不存在“未 commit 的边/点在恢复后可见”
- manifest epoch 单调递增（防止回滚）

