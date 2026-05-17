# T44: v2 M2 — CSR Segments + 显式 Compaction（读性能质变）

## 1. Context

T43（M1）已经提供：

- IDMap（I2E 持久化 / E2I 启动重建）
- WAL 图语义事件 + committed replay
- MemTable → commit 冻结 L0Run → Snapshot 读隔离

但 M1 的邻接读取仍然主要依赖 L0Runs（增量段），读放大随写入增长会迅速失控。M2 的目标是引入不可变磁盘段（CSR segment），并通过显式 compaction 把增量固化为高效的顺序访问布局。

## 2. Goals（M2 验收）

- 新增不可变磁盘段：`CsrSegment`（存 `.ndb` pages）
- `db.compact()` 显式触发：
  - 将一批 L0Runs（或 MemTable 的快照）flush/merge 为一个新 CSR segment
  - tombstone 清理：segment 内不写入已被 tombstone 的 key
- 读路径 merge：
  - `Snapshot = L0Runs (new→old) + CsrSegments (new→old)`
  - `neighbors(src, rel?)` 支持同时读取两类段并去重/过滤 tombstone
- crash-safe 原子切换：
  - 新 segment 落盘后才更新 manifest（见 T45）

## 3. Non-Goals

- 后台 compaction 默认不开（嵌入式环境不允许不可控 IO）；可选 feature 后置
- 二级属性索引、向量/FTS、入边索引（先 out-going）
- 完美“单一全局 CSR”——只做多段 CSR（LSM 风格）

## 4. CSR Segment Format（最小可实现）

### 4.1 Segment 概念

CSR segment 是“不可变 run”，包含某一时刻的边集合（已应用 tombstone），适合 mmap/顺序扫描。

### 4.2 Segment key（必须一致）

Edge sort key（宪法）：`(src_iid:u32, rel:u32, dst_iid:u32)`

### 4.3 页布局（建议）

一个 segment 最小包含：

- SegmentMeta page：
  - `segment_id:u64`
  - `min_src:u32` `max_src:u32`
  - `edge_count:u64`
  - `offsets_page_start:u64` `offsets_page_len:u32`
  - `edges_page_start:u64` `edges_page_len:u32`
  - `checksum:u32`（覆盖 meta + data 范围）
- Offsets（按 src）：
  - `offset[src-min_src .. max_src-min_src+2]`，u64（edge index）
- Edge data：
  - 紧凑数组（按 src 分组，组内按 (rel,dst) 排序）
  - record：`dst:u32, rel:u32`（8 bytes）

> 说明：M2 先不存属性；属性仍在 delta（WAL/L0Run）。后续在 M3/M4 再合并属性列式页。

## 5. Compaction 规则（显式）

### 5.1 输入

- 选择一批 L0Runs（从旧到新或新到旧都行，但规则必须固定）
- 可选：限制 run 数量/总边数，避免一次 compaction 太大

### 5.2 输出

- 生成一个新的 CSR segment（不可变）
- 生成新的 manifest（或在 manifest 追加新 segment、标记旧 segment/old runs 可回收）

### 5.3 Tombstone 处理（key-based）

- 任何被 tombstone 的边 key 不写入新 segment
- tombstone 本身不写入 segment；它的作用通过“过滤后不落盘”体现

## 6. Read Path（L0Runs + Segments）

`neighbors(src, rel?)` 读顺序：

1. 先读 L0Runs（new→old），产生候选边
2. 再读 CSR segments（new→old），产生候选边
3. 去重：
   - key 去重（HashSet/last-write-wins）
4. 过滤 tombstone：
   - 任何 run/segment 侧的 tombstone 集合都必须在 merge 前应用

> 注意：segment 不含 tombstone，所以 tombstone 只来自 L0Runs；M2 compaction 后 tombstone 会被“吸收”。

## 7. Testing Strategy

- correctness：
  - compaction 前后 neighbors 结果一致
  - tombstone 在 compaction 后不再出现
- crash safety（依赖 T45 定义的 manifest/atomic switch）：
  - compaction 过程中崩溃：要么看到旧段集，要么看到新段集，不能看到“半成品段集”
- perf：
  - traversal 读性能显著提升（数量级），基准见 T48

