# NervusDB 性能分析报告

## 当前 Benchmark 结果（100万条数据）

| 数据库 | 插入/秒 | S?? 查询/秒 | ??O 查询/秒 |
|--------|---------|-------------|-------------|
| **SQLite** | 940,956 | 343,695 | 376,022 |
| **redb (raw)** | 170,210 | 326,732 | 989,275 |
| **NervusDB** | 72,281 | 103,974 | 92,693 |

## 性能差距

- NervusDB 插入比 redb 慢 **2.4 倍**（72K vs 170K）
- NervusDB 查询比 redb 慢 **3-10 倍**

## 已完成的优化

### 1. WriteTableHandles 缓存（已实现）

```rust
pub(crate) struct WriteTableHandles<'txn> {
    pub spo: Table<'txn, (u64, u64, u64), ()>,
    pub sop: Table<'txn, (u64, u64, u64), ()>,
    // ... 共 7 个表句柄
}
```

- 效果：插入从 62,738 → 72,281（提升 15%）
- 问题：仍然比 redb 慢很多

## 仍存在的瓶颈

### 1. 五个索引表 vs 三个

NervusDB 维护 5 个索引表：SPO, SOP, POS, PSO, OSP

Benchmark 的 redb 只维护 3 个：
- SPO 主表
- subject_idx（S?? 查询）
- object_idx（??O 查询）

**每次插入写 5 个表 vs 3 个表，开销多 67%**

### 2. 字符串 intern 开销

每次插入 fact 需要：
1. 查 str_to_id 表看字符串是否存在
2. 如果不存在，写入 str_to_id 和 id_to_str 两个表

Benchmark 的 redb 和 SQLite 直接存字符串，没有这个开销。

### 3. 重复检查开销

```rust
// 每次插入前都要查一遍
if spo.get((s, p, o))?.is_some() {
    return Ok(false);  // 跳过重复
}
```

### 4. 查询时每次创建新事务

```rust
fn query(&self, ...) {
    let txn = db.begin_read()?;      // 每次查询都创建新事务
    let table = txn.open_table(...)?; // 每次都打开表
}
```

而 benchmark 的 redb 复用同一个读事务和表句柄。

## 代码位置

主要文件：
- `nervusdb-core/src/storage/disk.rs` - 磁盘存储实现
- `nervusdb-core/src/storage/mod.rs` - Hexastore trait 定义
- `nervusdb-core/src/lib.rs` - Database API
- `nervusdb-core/examples/bench_compare.rs` - Benchmark 代码

关键函数：
- `WriteTableHandles::insert_fact()` - 优化后的插入
- `WriteTableHandles::intern()` - 字符串 intern
- `DiskCursor::create()` - 查询游标创建

## 问题

1. **是否需要 5 个索引？** 能否减少到 3 个？
2. **字符串 intern 是否必要？** 有什么更高效的方式？
3. **如何优化查询？** 能否缓存读事务/表句柄？
4. **重复检查能否跳过？** 提供 `insert_unchecked` 选项？

## 运行 Benchmark

```bash
cargo run --example bench_compare -p nervusdb-core --release
```
