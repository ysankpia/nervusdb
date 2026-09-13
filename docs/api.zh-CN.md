# API 用法样例

按任务分组的可运行样例。**每一段都对应 `examples/` 下的一个文件，编译并运行过**，
不是示意代码——写这份文档时，正是「跑一遍」抓出了两处错误：`k_hop_subgraph` 的参数
个数，以及「写句柄存活时只读打开会失败」这条边界。

文件名与下文的顺序一一对应：

```text
examples/open_and_close.rs       examples/unique_constraints.rs
examples/crud.rs                 examples/read_snapshot.rs
examples/batch_writes.rs         examples/algorithms.rs
examples/explicit_transaction.rs examples/backup_and_integrity.rs
examples/cypher_queries.rs       examples/export.rs
```

运行其中任意一个：`cargo run --example crud`。

**CI 会把它们逐个跑一遍**（不是只编译）：`cargo check --all-targets` 能证明样例能
编译，但证明不了它跑起来是对的——一个每次都 panic、或输出错误结果的样例，编译阶段
照样通过。因此 CI 里有一条单独的步骤执行 `examples/` 下每个文件。

这一条是有来历的：样例曾经只被编译，直到有人真的去跑，才发现 `k_hop_subgraph` 的
参数个数写错、以及「写句柄存活时只读打开会失败」这条边界没被写进样例。编译通过而
跑不对，是这类文档最容易出、也最难发现的问题。

英文 API 名称保持原样，注释用中文。

签名以源码为准：入口在 [`src/lib.rs`](../src/lib.rs)，领域模型在
[`src/graph.rs`](../src/graph.rs)。完整的行为契约（而非用法）见
[AGENTS.md](../AGENTS.md) 与 [docs/cypher.md](cypher.md)。

---

## 1. 打开与关闭

```rust
use nervusdb::{NervusDb, NervusDbOptions, GraphError};

// 默认 4 MB 缓冲池（1024 帧）
let db = NervusDb::open("mydb.db")?;

// 按 MB 指定池大小
let db = NervusDb::open_with_pool_mb("mydb.db", 64)?;

// 完全控制
let db = NervusDb::open_with_options("mydb.db", NervusDbOptions {
    buffer_pool_frames: 4096,
    wal_auto_checkpoint_bytes: 64 * 1024 * 1024,
    read_only: false,
    max_transaction_actions: 4_000_000,
    // 队列触顶时把动作溢出到 WAL（默认 false）。见下方「事务动作队列」。
    spill_transaction_actions: false,
    // 库被别的句柄占用时最多等多少毫秒（默认 0 = 不等待，立即报错）。
    // 见 docs/concurrency.md：两个会话窗口先后写同一个库时的关键选项。
    lock_wait_ms: 0,
    ..Default::default()
})?;

// 只读句柄：取共享锁，多个读者可并存；所有写入口都会拒绝。
//
// 但**写句柄存活时只读打开会失败**（DatabaseLocked）——写者与读者互斥。
// 反过来也一样：只读句柄存活时无法取写句柄。
let ro = NervusDb::open_read_only("mydb.db")?;
assert!(ro.is_read_only());
```

**同一文件同时只能有一个写句柄**（跨进程也是）；第二个 `open` 返回
`GraphError::DatabaseLocked`。这不是限制，而是防「两个写者各自以为成功、第二个写入静默丢失」。

**写者与读者也互斥**：写句柄存活期间，`open_read_only` 同样失败；
只读句柄存活期间，`open` 也失败。多个只读句柄彼此可以并存。

只读打开要求 WAL 里没有待回放的页；若有，会明确报错并提示先用读写句柄打开一次，
而不是让你读到过期数据。

---

## 2. 增删查改

```rust
use std::collections::{HashMap, HashSet};
use nervusdb::{NervusDb, Value};

let db = NervusDb::open("mydb.db")?;

// 单条写入：自带一个事务，一次 fsync
let id = db.add_node(
    HashSet::from(["Person".to_string()]),
    HashMap::from([("name".to_string(), Value::from("林渊"))]),
)?;

let other = db.add_node(HashSet::new(), HashMap::new())?;
db.add_edge(id, other, "KNOWS", HashMap::new(), 1.0)?;

// 读：有损 API，存储错误被折叠成 None
if let Some(node) = db.get_node(id) {
    println!("{:?}", node.properties);
}

// 读：保留错误的 API，Ok(None) 只表示「确实不存在」
let node = db.try_get_node(id)?;
assert!(node.is_some());

// 更新属性
db.update_node_property(id, "age", 28i64)?;

// 删除（有损 vs 保留错误同前）
db.remove_edge(1)?;
db.remove_node(other)?;

// 计数与模式
assert_eq!(db.node_count(), 1);
println!("labels: {:?}", db.index_labels());
```

**读取时优先 `try_get_*`。** `get_node` / `get_edge` 是刻意保留的有损接口，
目的只是兼容早期签名；它们无法区分「不存在」与「页面读不出来」。

---

## 3. 批量写入（性能关键路径）

```rust
use std::collections::{HashMap, HashSet};
use nervusdb::{NervusDb, Value};

let db = NervusDb::open("mydb.db")?;

// 一个事务 = 一次 fsync。这是批量导入唯一正确的写法。
db.with_transaction(|tx| {
    for i in 0..100_000i64 {
        tx.add_node(
            HashSet::from(["Bulk".to_string()]),
            HashMap::from([("idx".to_string(), Value::from(i))]),
        )?;
    }
    Ok(())
})?;
```

`with_transaction` 的闭包返回 `Err` 时**整个事务回滚**，未提交数据绝不落库。

逐条自动提交（上面的 `add_node`）每条一次 fsync，比批量慢两到三个数量级。
两者产出的图完全一致——差别只在 fsync 次数：

```rust
let before = db.buffer_stats().wal_fsync_count;
db.with_transaction(|tx| { /* N 次写入 */ Ok(()) })?;
assert_eq!(db.buffer_stats().wal_fsync_count - before, 1);   // 恰好 1 次
```

### 事务动作队列有上限

事务会把全部动作留在内存里：实测每个节点动作约 **502 字节**、每条边约 **128 字节**。
默认上限 `DEFAULT_MAX_TRANSACTION_ACTIONS` = 400 万（约 812 MB 最坏情况）。
超过会**明确报错**：

```text
Transaction action queue is full (4000000 actions). ...
Commit in batches instead, or raise the limit deliberately with
NervusDbOptions::max_transaction_actions.
```

它不会自动分块——分块等于提交事务的一部分，会破坏「要么全做要么全不做」。

### 多会话/多进程：第二个句柄该等还是该报错

**嵌入式数据库一次只允许一个写者**（SQLite 默认同样如此）。两个会话窗口同时写
同一个库时，后来者默认**立即**拿到 `DatabaseLocked`。

若你的写入是**先后**发生（第二个窗口在第一个写完之后才来），把 `lock_wait_ms`
设成非零值让它等待：

```rust
let db = NervusDb::open_with_options("mydb.db", NervusDbOptions {
    // 最多等 3 秒；持有者释放后立即成功，超时仍是 DatabaseLocked
    lock_wait_ms: 3000,
    ..Default::default()
})?;
```

**它不能让两个写者并存**——只是把「立刻失败」变成「等一会儿，超时仍失败」。
真正的并发写需要版本可见性或服务器模型，见 `docs/concurrency.md`。

只读句柄同样受这个选项影响（等写者释放后打开）。

### 想跑更大的事务：让动作溢出到 WAL

默认行为（`spill_transaction_actions: false`）如上：触顶报错。若你确实需要单事务
超过上限，把它设为 `true`：

```rust
let db = NervusDb::open_with_options("mydb.db", NervusDbOptions {
    spill_transaction_actions: true,
    ..Default::default()
})?;
```

此时触顶不再报错，而是把已入队的动作写成 WAL 帧，只在内存里保留**每个动作 8 字节**
的位置索引。常驻内存从「≈502 字节 × 动作数」变成「一个窗口 + 8 字节 × 动作数」，
因此事务规模不再由内存上限决定。

**代价是三条，都真实存在：**

- 只要有事务把动作溢出在 WAL 里，**Checkpoint 会被拒绝**并返回错误。Checkpoint 会
  截断 WAL，而那些帧正是该事务尚未施加的动作。拒绝是**报错**而不是静默跳过——
  否则调用方会把「推迟了」当成「做完了」。
- 由这种情况**被推迟的自动 Checkpoint，不会让触发它的那次提交报错**。那次提交此时
  已经持久化；把它报成失败只会引诱调用方重试，而重试会产生重复数据。
- WAL 体积随事务增长，直到它提交或回滚。

---

## 4. 显式事务

需要「读—判断—写」在同一事务内完成时：

```rust
use std::collections::HashMap;
use nervusdb::{NervusDb, Value};

let db = NervusDb::open("mydb.db")?;

let mut tx = db.begin_transaction()?;
let a = tx.add_node(Default::default(), HashMap::new())?;
let b = tx.add_node(Default::default(), HashMap::new())?;
tx.add_edge(a, b, "LINK", HashMap::new(), 1.0)?;

// 提交前都可以反悔。不调用 commit 直接 drop 也会回滚。
tx.commit()?;
```

注意四个方法的签名是 `Result<(), GraphError>`（队列有上限，必须能报错）：
`remove_node`、`remove_edge`、`update_node_property`、`update_edge_property`。
调用时要加 `?`。

---

## 5. Cypher 查询

```rust
let db = nervusdb::NervusDb::open("mydb.db")?;

// 自动路由：读查询走共享锁，写语句走排他锁并提交 WAL
let rows = db.run_cypher("MATCH (n:Person) RETURN n.name AS name, n.age AS age")?;
for row in &rows.rows {
    println!("{:?}", row.values);
}

// 只要统计摘要（行数、创建/删除计数）
let stats = db.execute("UNWIND [1,2,3] AS i CREATE (n:Num {v: i})")?;
assert_eq!(stats.nodes_created, 3);

// 先看执行计划，不执行
let plan = db.run_cypher("EXPLAIN MATCH (n:Person) RETURN n")?;
println!("{:?}", plan.rows[0].values[0]);
```

`execute` 返回 `ExecuteResult`（无行集合）；`run_cypher` / `query_cypher` 返回
`CypherResultSet`（含 `columns` 与 `rows`）。要结果就用后者。

---

## 6. 唯一约束

```rust
let db = nervusdb::NervusDb::open("mydb.db")?;

// 声明前会先检查既有数据；已有重复会报错并指名冲突节点
db.create_unique_constraint("Person", "name")?;

// 约束在所有写路径上生效：Cypher、事务、单条 API
db.run_cypher("CREATE (:Person {name: 'alice'})")?;
assert!(db.run_cypher("CREATE (:Person {name: 'alice'})").is_err());
assert!(db.with_transaction(|tx| {
    tx.add_node(
        std::collections::HashSet::from(["Person".to_string()]),
        std::collections::HashMap::from([("name".to_string(), "alice".into())]),
    )?;
    Ok(())
}).is_err());

println!("{:?}", db.unique_constraints());
```

**约束跨重开存活。** 这条曾经是真实缺陷：约束只写在内存 catalog 里，重开后消失，
于是重复值被静默接受——比报错更危险，因为调用方会因此停止自己做去重。

---

## 7. 自洽读（快照）

```rust
let db = nervusdb::NervusDb::open("mydb.db")?;

// 快照存活期间持有读锁，因此其间所有读取看到的是同一个状态
let snapshot = db.read_snapshot();
if let Some(node) = snapshot.get_node(1)? {
    for eid in &node.outgoing {
        // 同一次快照里，邻接表引用的边必然读得到
        assert!(snapshot.get_edge(*eid)?.is_some());
    }
}
drop(snapshot);   // 释放，写者才能继续
```

**为什么需要它**：`get_node` 与 `get_edge` 各自取放读锁，一次多步遍历会横跨多个时刻。
中间发生的并发删除会让遍历「看到」一条已经读不出来的边——那不是引擎内部撕裂，
而是调用方把两个时刻的状态拼在了一起。

**代价要说清**：快照会阻塞写者。它给的是**正确性**，不是并行度；
真正的读写并行需要页级版本可见性（[ROADMAP](../ROADMAP.md) 第 1 项）。因此快照要短命，
并且**绝不能在持快照时调用写入口**（那会等一个自己持有的锁，自锁死）。

---

## 8. 图算法

```rust
use nervusdb::{Direction, NervusDb};

let db = NervusDb::open("mydb.db")?;

// 最短路（带权重）
if let Some((cost, path)) = db.dijkstra(1, 5, Some("ROAD")) {
    println!("cost={cost} path={path:?}");
}

// 无权重最短路
let path = db.bfs(1, 5, None);

// 环检测：深图不会栈溢出（内部是显式栈迭代）
let has_cycle = db.try_has_cycle()?;          // 保留错误
let cycles = db.try_find_cycles()?;

// PageRank：返回值按分数降序，且总和为 1.0
let scores = db.pagerank();
let tuned = db.pagerank_with(0.85, 100, 1e-6);

// 弱连通分量：把边视为无向，按规模降序
let components = db.weakly_connected_components();

// K 跳子图：只保留两端都在子图内的边
// 默认方向/边类型：k_hop_subgraph(start_id, k)
let sub = db.k_hop_subgraph(1, 2)?;
println!("nodes={} edges={}", sub.nodes.len(), sub.edges.len());

// 需要指定方向或边类型时用 _with 变体
let sub2 = db.k_hop_subgraph_with(1, 2, Direction::Outgoing, None)?;
```

全部走磁盘游标，**不会**把整张邻接表materialize到内存里。

---

## 9. 备份、空间与完整性

```rust
let db = nervusdb::NervusDb::open("mydb.db")?;

// 一致的在线副本：先 checkpoint 让主文件成为权威快照，再持写锁复制
let bytes = db.backup("snapshot.db")?;
println!("copied {bytes} bytes");

// 空间报告。注意：它**不截断文件**——页号是逻辑到物理的映射，截断会破坏映射
let report = db.vacuum()?;
println!("live={} reclaimable_prop_pages={}",
         report.nodes_live, report.free_property_pages);

// 结构化校验：只读，且**从不自动修复**
let report = db.integrity_check()?;
if !report.is_ok() {
    for issue in &report.issues {
        println!("{:?}: {}", issue.kind, issue.detail);
    }
}
```

`backup` 拒绝覆盖已存在的目标，也拒绝备份到自己身上——两者都会导致数据丢失。

---

## 10. 导出与可视化数据

```rust
let db = nervusdb::NervusDb::open("mydb.db")?;

// 逻辑导出：可回灌到新库的 Cypher 脚本
let mut out = Vec::new();
db.dump_cypher(&mut out)?;
println!("{}", String::from_utf8_lossy(&out));

// 结构化子图，供前端渲染
let export = db.export_subgraph(5_000)?;
println!("{}", export.to_json());
assert!(!export.truncated, "超过 limit 时 truncated 为 true，不会静默截断");
```

`export_subgraph` 超过上限会**明确设 `truncated = true`**，而不是悄悄少给数据。

---

## 11. C ABI

`src/c_api.rs` 对 C 暴露了 `nervusdb_open` / `nervusdb_close` / `nervusdb_execute`
等 `extern "C"` 符号，`#[no_mangle]` 导出。**没有头文件**：符号名与参数类型
以 `src/c_api.rs` 为准，这是目前唯一的规范来源。
