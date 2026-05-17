# Design T106: Checkpoint-on-Close (WAL Compaction)

> **Status**: Draft
> **Parent**: T100 (Architecture)
> **Risk**: Medium (durability & recovery)

## 0. The Real Problem

现在的 `.wal` 是“只增不减”的：

- 写入会不断追加 WAL tx（CreateNode/CreateEdge/Properties…）
- compaction 会写 `ManifestSwitch`/`Checkpoint`，但 **不会**裁剪旧 WAL

结果：长跑进程/反复写入后 WAL 会膨胀，打开/扫描成本也跟着涨。

## 1. Data Structures (What Actually Matters)

恢复依赖链（别幻想，按事实来）：

- **CSR segments** 存在 `.ndb`，但“当前 segment 列表”只在 WAL 的 `ManifestSwitch` 里
- **LabelInterner（name <-> id）** 目前只存在于 WAL 的 `CreateLabel` 里（`.ndb` 没有持久化）
- **L0 runs（含 edges/tombstones/properties）** 目前不落盘到 `.ndb`，只能靠 WAL replay 重建

结论：你想裁剪 WAL，必须保证：

1) “该裁掉的 tx”对应的数据已经在 `.ndb` 里且已 fsync  
2) “不能裁掉的元数据”（labels/manifest）必须保留（或迁移到 `.ndb`，这不是 T106）

## 2. MVP Rule (Lossless Only)

**Never break userspace / 数据不丢是硬门槛**：

- 只有当 `published_runs` 为空时，才允许做 WAL compaction。
  - 因为 runs 非空意味着还有数据（edges/tombstones/properties）只能靠 WAL replay
  - 尤其 properties 目前不进 `.ndb`（T103 已锁死规则）

如果 runs 非空：close 时只做最小动作（例如 fsync pager），不裁剪 WAL。

## 3. What “Checkpoint-on-Close” Means Here

关闭时做一个“WAL 快照”，把 WAL 重写成最小可恢复集：

- 单个 committed tx（一个 txid 就够）包含：
  - 当前 LabelInterner 的所有 `CreateLabel { name, label_id }`
  - 当前 segments 列表的 `ManifestSwitch { epoch, segments }`（若 segments 非空）
  - `Checkpoint { up_to_txid, epoch }`（当且仅当 runs 为空）

然后用原子 rename 替换旧 `.wal`，让 WAL 体积收敛。

## 4. API Surface

- 增加 `Db::close(self) -> Result<()>`：显式 close，允许返回错误
  - Drop 不做隐式 checkpoint（不把昂贵操作塞进析构）
- storage 层新增 `GraphEngine::checkpoint_on_close()`（内部调用）

## 5. Tests

- edge-only 写入（无 properties）：
  - 多次写入让 WAL 变大
  - `db.close()` 后 WAL 变小（被重写）
  - 重开 DB：邻接关系仍可读（从 `.ndb` segment + manifest 恢复）
- properties 存在：
  - `db.close()` 不应裁剪 WAL（因为 runs 非空）

