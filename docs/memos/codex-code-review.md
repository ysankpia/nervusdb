# NervusDB v2 — Codex Code Review（commit 34e5323c，2025-12-31）

审查方式（别自嗨，先讲清楚证据链）：

- 静态扫描：`rg`（panic/unwrap/unsafe/锁）
- 抽样深读：`nervusdb-v2-storage/src/engine.rs`、`nervusdb-v2-storage/src/api.rs`、`nervusdb-v2-storage/src/wal.rs`、`nervusdb-v2-storage/src/bulkload.rs`、`nervusdb-v2-storage/src/backup.rs`、`nervusdb-v2-query/src/executor.rs`、`nervusdb-pyo3/src/{db.rs,txn.rs}`
- 全量打包：repomix output `repomix-output.md`（outputId: `77c9f4b3a65a6e4e`）
- 最小验证：`cargo test`（通过，但有多处 `unused_*` warning）

> 你仓库里已经有一份更早的审查：`docs/memos/CODE_REVIEW_2025-12-28.md`。那份重点在“Query 白名单 + fail-fast”。我这份聚焦**新增/被忽略的硬伤**：UB、panic 边界、数据结构一致性。

---

## 0) Linus 的三问（先把脑子拧回来）

1. **这是现实问题还是臆想？**  
   现实问题：你现在有 **UB（未定义行为）** 和 **用户输入可触发 panic**。这不是“代码洁癖”，这是“你在生产里自杀”。
2. **有没有更简单的办法？**  
   有：把“会失败”的东西做成**数据结构保证不失败**，而不是在上层 `expect/unwrap`。特殊情况越少越好。
3. **会不会破坏任何东西？**  
   会：当前 Python binding 的生命周期处理可以把进程送进地狱；WAL/property 编码的 panic 也会把用户进程炸掉。你要的是数据库，不是定时炸弹。

---

## 【Core Judgment】

✅ 值得继续：v2 的内核主线（Pager/WAL/Manifest/Checkpoint + Snapshot + Compaction）方向是对的，结构也基本清晰。  
❌ 现在最大问题不是“功能不够”，而是**边界不硬**：不该 panic 的地方在 panic，不该 UB 的地方在 UB，不该语义漂移的地方在漂移。

---

## 【Key Insights】

- **Data Structure（最关键的数据关系）**：`MemTable(delta) -> L0Runs -> CSR segments`，一致性靠 `WAL + manifest/checkpoint`，读隔离靠“发布快照”（`Arc<Vec<...>>`）。
- **Complexity（可以砍掉的复杂度）**：你在多个地方用 `unwrap/expect/panic` 兜底，等于把“错误处理”变成“随机崩溃”。这不是复杂度低，这是**质量低**。
- **Risk Point（最大破坏风险）**：Python binding 的 `transmute` 把 Rust 生命周期规则当笑话；BulkLoader 的 label/rel type ID 映射不一致会制造**静默错误结果**（比崩溃更烂）。

---

## 【Taste Rating】

- 🟢 **nervusdb-v2-storage（内核主线）**：整体“内核化”思路对，边界能讲清。  
- 🟡 **nervusdb-v2-query（执行器/计划）**：MVP 可用，但要持续坚持白名单 + fail-fast，否则会腐烂成一坨 if/else。  
- 🔴 **nervusdb-pyo3（Python binding）**：现在是**内存安全事故现场**，不是“有点 hack”。  
- 🟡 **backup/bulkload（生态能力）**：功能看起来在，但关键语义（checkpoint、ID 映射）没做硬，会误导用户以为“可用”。

---

## P0（立刻修，不然别叫数据库）

### P0.1 `nervusdb-pyo3`：`transmute` 伪造 `'static`，在 Python 侧可触发 UB

位置：

- `nervusdb-pyo3/src/txn.rs`：`unsafe { transmute::<RustWriteTxn<'_>, RustWriteTxn<'static>>(txn) }`
- `nervusdb-pyo3/src/db.rs`：`Db.close()` 会 `self.inner.take()` 丢掉 RustDb

为什么是硬伤：

- Python 里**完全可以**：`txn = db.begin_write(); db.close(); txn.commit()`  
  这会让 `txn` 里那个被你“延寿”的引用指向已释放的对象——这不是 panic，这是 **未定义行为**。

最小修复方向（别过度设计）：

1. **把 RustDb 的所有权变成 `Arc`**：`Db { inner: Option<Arc<RustDb>> }`，`WriteTxn` 持有 `Arc<RustDb>`，从根上消除“owner 被 close 掉”的可能。
2. 或者（更粗暴但可接受）：在 Python 层**禁止 close**，或者 close 时检查是否有活跃 txn（引用计数/计数器），有就报错。
3. 最好别再用 `transmute`：如果 Rust 侧把 `Db`/`GraphEngine` 设计为可克隆的 `Arc`，`begin_write()` 返回的 txn 可以拥有 `Arc`，生命周期自然正确。

### P0.2 `BulkLoader`：Label/RelType ID 映射不一致，导致语义错而不自知

位置：

- `nervusdb-v2-storage/src/bulkload.rs`：`build_idmap_and_labels()`、`build_segments()`、`write_properties()`、`initialize_wal()` 里反复新建 `LabelInterner`

为什么是硬伤：

- `GraphEngine` 把 label_id 和 rel_type_id 混用同一个 interner（至少目前如此：`get_or_create_rel_type()` 复用 label interner）。
- 但 `BulkLoader` 用不同的 interner 给 rel_type 分配 ID：同一个字符串在不同阶段拿到的 ID **可能不同**。
- 结果就是：导入的数据在磁盘上“看起来有边”，但查询层按 rel_type 过滤会失效，甚至 label/rel 语义串台。这是**静默错误**，最恶心。

最小修复方向：

- BulkLoader 全流程使用**同一个** interner（label + rel_type 一个 namespace，跟 engine 保持一致），并保证 WAL/segments/properties 三者使用同一套 ID 映射。

### P0.3 用户输入可触发 panic：WAL/property 编码长度检查用 `expect/panic`

位置：

- `nervusdb-v2-storage/src/wal.rs`：`panic!("label name too long")`、`panic!("key too long")`、以及 `map_err(...).unwrap()`  
- `nervusdb-v2-storage/src/property.rs`：`expect("... length should fit in u32")`

为什么是硬伤：

- 用户完全能写入超长 key/string/blob/list/map。你的代码现在会直接 panic，把用户进程炸了。  
  “Never break userspace” 在这里就是：**不要因为用户输入把人家进程打爆**。

最小修复方向：

- 把“长度上限”变成公开约束（文档 + 错误返回），并在写入边界（set_property/prepare/commit 前）做检查，返回 `Error::WalRecordTooLarge`/`Error::Serialization` 等，而不是 panic。

### P0.4 `GraphStore::snapshot()` 设计逼出 `expect`：你在 `snapshot()` 里 panic

位置：

- `nervusdb-v2-storage/src/api.rs`：`scan_i2e_records().expect("...")`

为什么是硬伤：

- `GraphStore::snapshot()` 不返回 `Result`，导致你只能 panic。  
  这不是“代码风格问题”，这是 API 边界设计把你逼进墙角。

最小修复方向（不破坏现有 trait）：

- 让 `GraphEngine::open()` 把 `i2e` 一次性加载并保存到引擎状态（发布快照里），保证 `snapshot()` **不做 I/O 且不会失败**。  
  这是“好品味”：把失败从热路径挪到初始化，消灭特殊情况。

### P0.5 `BackupManager::get_checkpoint_info()` 现在是假的

位置：

- `nervusdb-v2-storage/src/backup.rs`：`get_checkpoint_info()` TODO，直接返回全 0

为什么是硬伤：

- 你叫它“Online Backup API / Hot snapshot”，但 checkpoint 信息是假的，那备份一致性就是假的。  
  这会把用户的数据一致性当玩具。

最小修复方向：

- 读 WAL 里最新 `Checkpoint/ManifestSwitch`（或者直接复用引擎里 `checkpoint_txid/epoch` 的权威状态），并只拷贝 WAL 的安全区间。

---

## P1（中期必须收口，否则会慢慢腐烂）

### P1.1 文档/实现漂移：`docs/tasks.md`/设计文档与现实不一致

现象：

- 任务表里 T202/T203 标成 Plan，但仓库里已经有 bulk import/hnsw 的实现与测试痕迹（而且有关键语义缺口）。  
  这会导致“以为 Done 实际不可靠”的发布事故。

建议：

- 要么把任务状态改成 WIP/Partial（不改 `docs/spec.md`，只改任务表/说明），要么把实现补齐到能对外承诺的程度。

### P1.2 `LabelInterner::merge()` 语义不对（虽未用，但这是坏味道）

位置：

- `nervusdb-v2-storage/src/label_interner.rs`：`merge()` 直接拷贝对方的 `(name, id)`，ID 空间冲突时会产生错误映射。

建议：

- 既然没用，要么删掉，要么实现成“按 name 合并并重新分配 ID”，别留这种会害人的 API。

### P1.3 锁与 poison：大量 `lock().unwrap()` 让 panic 成为“系统行为”

位置：

- `nervusdb-v2-storage/src/engine.rs`、`nervusdb-v2-storage/src/api.rs` 等

建议：

- 至少在对外 API 边界把 poison 转成 `Error`，或者换成不 poison 的锁实现（如果你愿意引入依赖）。核心原则：**别让一次 panic 把整个 DB 变成“必崩”状态**。

### P1.4 Query 写路径的 external_id 生成策略很可疑

位置：

- `nervusdb-v2-query/src/executor.rs`：`execute_create()` 用时间戳拼 external_id

问题：

- 这是不可复现、不可控、可能冲突的 ID 策略。你最终肯定要把 external_id 当成用户输入或严格生成的东西。

建议：

- 现在就把语义钉死：CREATE 不提供 external_id 时，明确规则（比如内部单调序列并回写可见），别用时间戳这种“看起来能跑”的烂招。

---

## P2（品味/性能/可维护性）

- `nervusdb-v2-query` 执行器大量 `Box<dyn Iterator>` 动态分发：MVP 可以，但别在性能目标下假装它“免费”。  
- `Row` 用 `Vec<(String, Value)>` 线性查找：数据结构上是可解释的（小 row），但你需要确保不会默默变成大 row 热路径。
- `cargo test` 有多处 `unused_imports/unused_variables` warning：这类漂移会逐渐污染代码质量，最好在 CI 里收紧（至少新代码别新增 warning）。

---

## Gatekeeper（我跑过的验证）

- `cargo test`：通过（但有 warning，详见命令输出）

---

## 我建议你现在就做的“最小整改清单”（按顺序）

1. **先把 PyO3 UB 修掉**（这是“事故级”）：用 `Arc`/禁 close/活跃 txn 计数，别再 `transmute` 自我安慰。
2. **修 BulkLoader 的 ID 映射一致性**：一个 interner 贯穿全流程，写入/段/属性/WAL 一致。
3. **把 panic 变成 error**：WAL/property 编码长度检查做成可控错误路径。
4. **让 snapshot 热路径不做 I/O 且不 panic**：把 i2e 等必要状态搬到 open 时加载并发布。
5. **补一条针对 BulkLoader rel_type 的测试**：导入后按 rel_type 过滤必须命中，否则你永远不知道什么时候又坏了。

