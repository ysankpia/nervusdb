# T42: v2 M0 — Pager + WAL Replay（Kernel 可验证内核）

## 1. Context

T40/T41 已确定 v2 “宪法”：

- v2 不兼容 v1：新 crate / 新 API / `.ndb + .wal`
- `Single Writer + Snapshot Readers`
- WAL 即 delta 的持久化形式（不搞双日志）
- LSM Graph：MemTable → 冻结 L0 runs → 多 CSR segments（M2）

T42 的目标不是“图数据库”，而是先把 v2 内核最硬的地基写对：**页式文件 + redo WAL + replay**，并且能被测试验证（含崩溃恢复模型）。

## 2. Goals（M0 验收）

### 2.1 功能

- `.ndb` page store：
  - 固定页大小 `PAGE_SIZE = 8192`
  - open/create
  - allocate/free page id
  - read/write page by id
- `.wal` redo log：
  - append records（可校验：len+crc）
  - `BeginTx/CommitTx` 事务边界（单写者）
  - replay：启动时重放已提交记录，重建内存状态并把必要的 page 变更落回 `.ndb`

### 2.2 正确性（必须写死）

- 崩溃一致性模型（M0 简化版）：
  - “已提交事务”的 WAL 记录必须在重启后可重放
  - 未提交事务必须完全不可见（重放时丢弃）
- durability：
  - 默认 `Full`：每次 commit `fsync` WAL（`.wal`）
  - `.ndb` 的刷盘顺序必须受控（由 checkpoint/flush 逻辑决定）

### 2.3 非目标

- 不实现 snapshot readers（M1 才引入冻结 L0 runs）
- 不实现 CSR / compaction（M2）
- 不实现属性 schema（M1 只在 log/memtable）

## 3. Deliverables

### 3.1 新增 crate（按 T41）

最小落地只建 v2 存储 crate：

- `nervusdb-v2-storage/`
  - 只负责 pager/wal/allocator/replay（不包含 query）
  - 对外暴露最小 API（M0 先暴露 page-level API，M1 再扩图语义）

> 其他 v2 crates（`nervusdb-v2`, `nervusdb-v2-query`, `nervusdb-v2-cli`）此任务不创建，避免范围失控。

## 4. API（M0）

M0 的 API 只服务“内核验证”，不做稳定承诺。

```text
Pager::open(path) -> Pager
Pager::allocate_page() -> PageId
Pager::free_page(PageId)
Pager::read_page(PageId) -> [u8; PAGE_SIZE]
Pager::write_page(PageId, [u8; PAGE_SIZE])
Pager::sync()   // 可控刷盘点

Wal::open(path) -> Wal
Wal::append(record)
Wal::fsync()
Wal::replay(visitor)  // 只回放已提交事务
```

## 5. File Format（M0 最小集合）

### 5.1 `.ndb` Meta Page（Page 0）

固定 8KB，前 64~128 bytes 为 header，其余预留。

- magic：`"NERVUSDBv2\0\0\0\0\0"`（16 bytes）
- version：`u32 major/minor`
- page_size：`u64`（固定 8192）
- freelist/bitmap root page id：`u64`（M0 可先固定为 page 1）
- next_page_id：`u64`
- checksum/epoch（预留）

### 5.2 页分配（M0）

M0 先用单层 bitmap（page 1）：

- 每 bit 表示一个 page 是否已分配
- 需要定义“page 0/1 永久占用”
- 允许文件增长：当 bitmap 扫完无空位时扩容 bitmap 或增长 file（M0 可先实现“增长 file + 追加 bitmap pages”的最小策略）

## 6. WAL Format（M0）

### 6.1 Record Envelope

```
[len:u32][crc32:u32][type:u8][payload...]
```

`crc32` 覆盖 `[type+payload]`（不含 len/crc 自身）。

### 6.2 Record Types（M0）

仅支持页级操作即可完成 M0 验证：

- `BeginTx{txid:u64}`
- `CommitTx{txid:u64}`
- `PageWrite{page_id:u64, page_bytes:[u8;8192]}`
- `PageFree{page_id:u64}`（可选，M0 也可暂不回放 free）

> M1 才引入图语义事件（CreateNode/CreateEdge/...），但底层仍会映射为 page writes。

### 6.3 Replay Rules

- 只重放 **BeginTx..CommitTx** 完整闭环的事务
- 不完整事务直接丢弃
- 重放顺序严格按 WAL 追加顺序

## 7. Testing Strategy（M0 必须有）

- 单元测试：
  - record 编解码 + crc 校验
  - bitmap allocator 边界（0/1 页保留、跨字边界）
- 集成测试：
  - 写 N 次 `PageWrite`，commit，关闭进程（正常退出），重启 replay 后校验 `.ndb`
  - “模拟崩溃”：写入 WAL 但不 commit（或写一半 record），重启后保证不可见/忽略损坏尾部

> 真实 `kill -9` 类测试可以在后续单独任务加 crash harness（类似 v1 的 crash-gate），但 M0 至少要有“非正常关闭 + replay”覆盖。

## 8. Risks / Open Questions（本任务结束前必须消除）

- `.ndb` 的 mmap 写路径 vs 普通 `File::write_at`：M0 先用最直白的 `File` I/O + `pread/pwrite`，不要急着 mmap（减少变量）；mmap 在 M1 引入。
- flush 顺序：要明确“WAL fsync 在前，`.ndb` flush 在后”的约束点（checkpoint）。
- 变长文件增长：allocator 扫描成本与增长策略，M0 先保证正确，性能后置。

## 9. Acceptance Criteria（明确到一条命令）

- `cargo test -p nervusdb-v2-storage` 全绿
- 关键集成测试通过：replay 后 `.ndb` 内容与预期一致，尾部损坏 WAL 不导致 panic/数据污染

