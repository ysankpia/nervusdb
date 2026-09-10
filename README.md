# GraphLite-RS (图数据库界的 SQLite)

> **GraphLite-RS** 是一个采用现代 Rust 构建的嵌入式、单文件持久化属性图数据库引擎。
> 它融合了 **SQLite 的轻量零依赖与单文件部署体验** 与 **原生图数据库（如 Neo4j）的定长记录磁盘免索引邻接性能**，彻底打破超大图谱常驻内存瓶颈。

> ⚠️ **当前为 `v1.0.0-rc.1` 预发布版本。** 核心引擎、Cypher 1.0、图算法与生产安全防线均已实现并通过 91 项测试，
> 但仍有若干已知生产缺口（单句柄限制、数据页无校验和、超大批量需分块等）。
> **上线前请先阅读 [已知限制](ROADMAP.md#next-planned)**，不要在未评估这些限制的情况下用于生产。

## 📊 实测数据一览

下表是**本项目开发过程中「优化前 → 优化后」的对照**（不是两个发布版本之间的对比）：

| 指标                                  | 优化前                            | 优化后                                             |
| ------------------------------------- | --------------------------------- | -------------------------------------------------- |
| 40,000 实体落盘体积（属性净长 ~150B） | 162 MB（每实体独占一整张 4KB 页） | **7.90 MB**（压缩 **19.8×**）                      |
| 每实体均摊体积                        | ~4096 B                           | **207 B**                                          |
| 批量写入吞吐                          | ~180 ops/s（逐条自动提交）        | **32 万 ~ 143 万 ops/s**（视属性载荷与介质，见下） |
| 单事务 WAL fsync 次数                 | 每条记录 1 次                     | **整个事务恰好 1 次**                              |
| 属性记录框架开销                      | ~28 B/实体（bincode HashMap）     | **~6 B/实体**（varint 紧凑编码）                   |

> 实测口径：10,000 节点 + 30,000 边，节点与边各带一条 ~150 字节字符串属性；
> 「优化前基线」= 实体数 × 4096B，即早期「每实体整页」的属性布局。

### 吞吐实测（可复现）

> **测量条件**：Apple M4（10 核）/ 16GB / macOS 15.7.5 / 内置 SSD；
> 单事务批量写入，`cargo bench --bench throughput`（release 构建）。
> 所有数字由仓库内 `benches/` 的基准程序产出，**每个场景都会打印自己的配置**。
> 复现命令与规模开关见下方「性能基准」一节。

节点写入（`Entity` 标签 + 2 个属性：整数 `idx` 与 8 位数字字符串 `name`）：

| 场景                         | 规模    | 缓冲池 | 吞吐                |
| ---------------------------- | ------- | ------ | ------------------- |
| 文件存储节点写入             | 1000 万 | 64MB   | **219,000 ops/s**   |
| 纯内存节点写入               | 100 万  | 64MB   | **577,000 ops/s**   |
| 文件存储节点写入             | 100 万  | 64MB   | **547,000 ops/s**   |
| 纯内存节点写入（**无属性**） | 100 万  | 64MB   | **1,430,000 ops/s** |

边写入（`1M 节点 + 4M 离散随机边`，边:节点 = 4:1）：

| 缓冲池            | 吞吐              |
| ----------------- | ----------------- |
| 64MB（16,384 帧） | 322,000 ops/s     |
| 1MB（256 帧）     | **906,000 ops/s** |

> **注意上表的反常**：此场景下**小池更快**。这不是缺陷——判别的关键在于
> 分块。把同一 400 万边工作负载拆成多个事务后，趋势立即**反转**为单调递增
> （256 帧 165k → 16384 帧 420k，且大池零溢出）。
> 根因是**单个巨型事务会在内存中累积全部待执行动作**：400 万边时进程 RSS
> 从 897MB 涨到 1104MB。大池在此基础上再占更多内存，两者争抢内存带宽。
> 因此：**超大批量写入应当分块提交**（见 ROADMAP「有界内存批量规划」）。

**性能数字与口径的关系**：无属性写入可达 143 万 ops/s，而带 2 个属性时降至
57 万。引用任何吞吐数字时必须同时说明属性载荷，否则比较无效。

### 边写入局部性

千万级离散边写入曾出现吞吐随规模衰减。根因不在缓存命中率，而是三处结构性缺陷：

| 缺陷                   | 影响                                                                                                         | 修复                                                                                 |
| ---------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| 逐条边织网             | 每条边分别触碰源页/目标页/旧首边页；受限内存下同页反复换出，每次 miss 向 WAL 溢出一整张 4KB 页（**假溢出**） | **两阶段批量织网**：节点页按逻辑页排序各读一次，链指针在内存推导，边记录分区顺序写出 |
| `LRUReplacer` 线性扫描 | `pin`/`unpin`/候选选取均为 **O(池容量)**，池越大单次页操作越慢                                               | 改为侵入式双向链表，全部 **O(1)**                                                    |
| 页目录页可被数据页置换 | 每次节点/边寻址都要重走目录链                                                                                | Page 0 与各级目录页登记为**置换豁免**                                                |

实测（1M 节点 + 4M 离散边，边:节点 = 4:1）：

| 缓冲池          | 优化前       | 优化后                 | spill/边         |
| --------------- | ------------ | ---------------------- | ---------------- |
| 1024 帧（4MB）  | 58,000 ops/s | **~200,000 ops/s**     | 1.20 → **0.045** |
| 256 帧（1MB）   | 84,000 ops/s | **~310,000 ops/s**     | 1.66 → **0.02**  |
| 8192 帧（32MB） | —            | **~260,000**（不衰减） | **0.031**        |

> `spill/边` 由 1.20 降至 0.031 即「假溢出」被消除；剩余 miss 属**真实容量不足**
> （4M 边记录页 15625 页 + 1M 节点页 7813 页 ≈ 91MB，远超 4MB 池），非结构缺陷。

---

## 🌟 核心架构亮点

### 1. 4KB 磁盘分页与 Buffer Pool 换页引擎 (彻底破除 RAM 瓶颈)

- **物理分页存储**：数据文件按 4096 字节（4KB）划分，支持物理 Paged I/O。
- **BufferPoolManager 缓冲池**：
  - 内存占用严格受控在可配置阈值内（默认 1024 帧 = 4MB，可调至 512 帧 = 2MB、256 帧 = 1MB）；
  - 采用标准 **LRU (Least Recently Used)** 换页置换淘汰算法；
  - 严谨的 **Page Pin/Unpin 引用计数** 与 **Dirty 脏页标记**，按需主动/惰性刷盘；
  - 任何图操作均按需通过 Buffer Pool 调取对应磁盘页，常驻内存永远受控在设定阈值之内，即使面对上百 GB 乃至数百 GB 的大图也能稳定运行。

### 2. STEAL 溢出与事务级 WAL 撤销恢复 (大事务不再受内存限制)

SQLite 风格的 **STEAL + WAL** 策略，让单事务修改的物理页数量可以远超缓冲池容量：

- **溢出暂存**：当事务进行中缓冲池帧全部用尽，未提交脏页会作为 redo 帧追加进页级 WAL，并在内存中维护一个**页位置索引**（每页 24 字节，性质等同 SQLite 的 wal-index），随后即可安全换出；
- **回滚自愈**：每个未提交页在首次被修改时记录**基线位置**；回滚时按基线还原页内容与索引位置，未提交的 redo 帧因缺少 `TxCommit` 而永远被恢复流程忽略 —— **主库文件绝不会被未提交数据污染**；
- **最小工作集水位**：`MIN_SPILL_FRAMES = 16`。缓冲池低于该水位时保持严格 NO-STEAL 防线（帧全部为未提交脏页即硬报错），与 SQLite 的最小页缓存策略一致；
- **严格两文件**：整个数据库始终只有 `{path}` 与 `{path}.wal` 两个文件，溢出不会落地第三个临时文件。

### 3. 单槽属性页紧凑存储 (Slotted Property Pages)

变长属性不再「每条独占一整张 4KB 页」，而是紧凑打包装入共享的开槽页：

- **页内布局**：`Header(24B) | Slot Array(向下生长) | free | Payload(向上生长)`。每条记录由 4 字节槽位描述符 `[offset u16 | len u16]` 定位，`len = 0` 表示死槽；
- **紧凑记录编码**：`PropCodec` 以 varint 计数 + 定长 key 前缀 + 1 字节类型 tag 编码，整数走 ZigZag，取代 `bincode(HashMap)` 约 28 字节/实体的纯框架开销，降至约 6 字节/实体；属性 key **不写入 StringDict**，避免字典膨胀溢出；
- **1KB 分界**：单条记录 ≤1KB 内联进槽位，>1KB 自动走 `PropertyPage` 溢出链（11KB 大文本路径完全保留）；
- **槽位复用与压实**：删除只标死槽；页满时原地 `compact` 回收碎片；整页腾空后挂入 `first_free_prop_page` 回收链。写入位点由有界提示环 `prop_page_hint`（上限 32 项）+ 向后探测 64 页维护，**只存页号，不含任何图拓扑**；
- **属性指针格式**：`NodeRecord` / `EdgeRecord` 的 `prop_page_id` 升级为**打包指针**——高 24 位 `PageId` + 低 8 位 `SlotId`（`0` = 无属性，槽 `0xFF` = 溢出链）。记录本身仍是定长 32B / 64B，$O(1)$ 寻址与记录布局不受影响。

### 4. 定长紧凑记录与磁盘免索引邻接 (Fixed-Size Record & Disk Adjacency)

- **$O(1)$ 磁盘物理直接寻址**（无需任何二级页表）：
  - **NodeRecord（定长 32 字节）**：`in_use (1B)` + `reserved (3B)` + `label_id (4B)` + `first_outgoing_edge_id (8B)` + `first_incoming_edge_id (8B)` + `prop_ptr (4B)` + `inline_prop_val (4B)`。每页精准容纳 128 条记录。
  - **EdgeRecord（定长 64 字节）**：`in_use (1B)` + `reserved (3B)` + `edge_type_id (4B)` + `prop_ptr (4B)` + `reserved2 (4B)` + `src_id (8B)` + `dst_id (8B)` + `weight (8B)` + `src_prev_edge_id (8B)` + `src_next_edge_id (8B)` + `dst_next_edge_id (8B)`。每页精准容纳 64 条记录。
  - **直接寻址算法**：
    $$\text{PageId} = \text{BasePage} + \frac{N \times \text{sizeof(Record)}}{4096}$$
    $$\text{Offset} = (N \times \text{sizeof(Record)}) \pmod{4096}$$
- **Freelist 空闲槽位复用**：在文件头 Page 0 维护 `first_free_node_id` 与 `first_free_edge_id`，删除记录时将其作为链表节点串入空闲链表，新增记录时优先弹出空闲槽位复用，杜绝磁盘稀疏空洞。
- **磁盘双向双环链表邻接**：每条边与其源节点的出边和目的节点的入边链表相连，图遍历顺着磁盘指针逐页载入，杜绝全表扫描。

### 5. 显式批量事务 (Explicit Batched Transactions)

- **Rust**：`db.with_transaction(|tx| { ... })` —— 闭包成功自动 commit，返回 `Err` 自动 rollback；
- **Python**：`with db.begin_transaction() as tx:` 上下文管理器，异常自动回滚；
- **Node.js**：`const tx = db.beginTransaction(); ...; tx.commit()`；
- **组提交语义**：整个事务内所有脏页在 `commit()` 时批量追加 WAL 并**只做一次 fsync**。`BufferStats::wal_fsync_count` 可观测该契约——脚本化验证断言批量提交的 fsync 增量**恰好为 1**。

### 6. 统一单文件物理存储与页级 WAL (Page-Level WAL)

- **单数据文件架构**：彻底切除任何内存图双写与多文件分散，统一为主数据文件 `{path}`（纯 4KB 物理分页数据文件）与预写日志 `{path}.wal`；
- **页级 WAL 记录**：WAL 日志记录标准物理页修改帧 `WalRecord::PageWrite { tx_id, page_id, data: 4KB, crc32 }`，事务提交时只落盘 WAL，Checkpoint 时将脏页同步回写至主文件并截断 WAL，崩溃自愈通过直接重放页物理数据无损恢复；
- **并发粒度下沉为页级门闩 (Page-Level Latches)**：彻底移除顶层针对整个图的粗暴独占大锁，Buffer Pool 中每个 Frame 享有独立读写保护，支持多线程并发遍历不同磁盘页。

### 7. 存储格式版本与旧库迁移

物理格式版本 `DB_PAGE_VERSION = 2`（开槽属性页）。早期「每实体整页」的属性布局与本版本**不兼容**，打开旧文件会返回明确错误而非静默误读：

```bash
# 旧版本环境导出逻辑数据
graphlite-cli old.db
graphlite> .dump old_graph.cypher

# 本版本环境导入到全新数据库
graphlite-cli new.db < old_graph.cypher
```

`.dump` 以 `CREATE` + `SET` 语句导出，回灌到既有节点时是**替换**语义，因此天然幂等。

### 8. 生产安全保证（排他锁 / 完整性校验 / 错误不静默）

三处「故障时静默出错」的路径已封堵。全部先经实测复现再修复：

| 曾经的失效       | 实测现象                                                                       | 现在的行为                                                               |
| ---------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| 无进程间互斥     | 两个句柄都报成功，第二次写入**静默丢失**（两写只活一个，零错误）               | `open` 对主数据文件取排他锁；第二个句柄收到 `GraphError::DatabaseLocked` |
| 主数据文件无校验 | 破坏一张 4KB 页后：打开成功、无错误、**200 个节点中 156 个属性静默错误或丢失** | `integrity_check()` 报出问题；`verify()` 返回带诊断的 `Err`              |
| 读错误被折叠     | `get_node` 用 `.ok().flatten()`，损坏被伪装成「节点不存在」                    | 新增 `try_get_node` / `try_get_edge` 保留错误；旧 API 标注为有损         |

- **锁的范围**：直接锁在 `{path}` 上（`std::fs::File::try_lock`，Rust 1.89+ 标准库，**零新依赖**），不产生 `.lock` 边车文件，「严格两文件」不变量不受影响。跨进程与同进程一律互斥。
- **加锁时序**：锁在 `StorageEngine::open` **之前**获取——因为 WAL 回放会写主文件，之后再锁已经来不及。
- **完整性校验原理**：度数守恒 oracle。链口径的每节点度数由「沿磁盘链指针走出」得到，期望度数由「独立扫描 edge id 空间」得到，两者必须相等。因此能抓出**只破坏链指针、计数仍自洽**的损坏——这正是第一版漏掉的一类。
- **只报告不自动修复**：修复策略需单独设计并显式授权。
- **锁中毒不再 panic**：71 处 `lock().unwrap()` / `expect("Lock poisoned")` 改为 `sync_ext` 的中毒恢复访问器。这些锁保护的是可重建的派生状态，恢复比重启进程更合适。

用法：

```rust
let db = GraphLite::open("mydb.db")?;   // 已被占用则返回 DatabaseLocked
db.verify()?;                            // 结构自洽性探针
let node = db.try_get_node(42)?;         // 保留存储错误；get_node 会把错误折叠为 None
```

### 9. 原生 Cypher 字符串查询引擎 (对齐 SQLite SQL 能力)

完整的词法（Lexer）、语法（Parser）与执行算子（Executor），支持直接执行标准 Cypher 字符串：

- **CREATE 变更**（支持多标签与多段路径）：
  ```cypher
  CREATE (a:Person:Engineer {name: "Alice", age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: "Bob", age: 32})
  ```
- **MATCH ... WHERE ... RETURN ... ORDER BY / SKIP / LIMIT 查询**：
  ```cypher
  MATCH (a:Person)-[:KNOWS]->(b:Person)
  WHERE b.age > 20 AND a:Engineer
  RETURN a.name, b.name, b.age
  ORDER BY b.age DESC SKIP 5 LIMIT 10
  ```
- **SET 属性更新与标签追加**：
  ```cypher
  MATCH (n:Person {name: "Alice"}) SET n.age = 31, n.city = "Beijing"
  MATCH (n:Person {name: "Alice"}) SET n:Employee
  ```
- **聚合函数**（支持分组与 `AS` 别名）：
  ```cypher
  MATCH (e:Emp) RETURN e.dept AS dept, count(e), sum(e.salary), avg(e.salary), min(e.salary), max(e.salary)
  ```
- **多跳变长路径与无向边匹配**：
  ```cypher
  MATCH (a)-[:KNOWS*1..3]->(b)
  MATCH (a)-[:KNOWS]-(b)
  ```
- **多模式匹配（跨模式按共享变量连接）**：
  ```cypher
  MATCH (a)-[:R]->(b), (b)-[:R]->(c) RETURN a, b, c
  ```
- **DELETE / DETACH DELETE 删除**：
  ```cypher
  MATCH (n:Person {name: "Alice"}) DETACH DELETE n
  MATCH (a)-[r:KNOWS]->(b) DELETE r
  ```
  > `DELETE` 删除仍有关联边的节点会显式报错并提示改用 `DETACH DELETE`，绝不静默丢失数据。

### 10. 企业级图算法引擎 (Graph Analytics)

- **BFS 最短路径 / Dijkstra 带权最短路**：纯磁盘邻接流式遍历；
- **PageRank 阻尼迭代**：可配置 `damping_factor` / `max_iterations` / `tolerance`，支持悬挂节点质量再分配，分数归一为 1.0；
- **弱连通分量 (WCC)**：并查集划分社群与孤岛检测，按分量规模降序返回；
- **K-Hop 局部子图提取**：抽取指定节点 K 步内的节点与边结构，支持方向与关系类型过滤；
- **有向环路检测**：三色标记法，支持全图环路枚举。

### 11. 属性与标签二级索引加速 (Secondary Indexing)

- **标签索引 (Label Index)**：`Label -> BTreeSet<NodeId>` 倒排集合；
- **属性索引 (Property Index)**：`(Label, PropKey) -> BTreeMap<Value, BTreeSet<NodeId>>`；
- **索引感知查询优化器**：执行包含标签或等值属性定位的查询时，自动优先命中二级索引获取起始点候选集，**严禁全图扫表**；
- **索引自愈**：属性更新 / 标签追加 / 事务失败后索引自动保持一致，冷重启后按需重建，杜绝陈旧索引幻读。

### 12. 交互式终端 REPL 客户端 (媲美 sqlite3 CLI)

- 启动：`cargo run --bin graphlite-cli -- mydb.db`
- 交互式提示符 `graphlite> `，续行提示符 `...> `，支持多行输入与分号终止（引号内的分号不会提前终结语句）；
- 打印对齐美观的 **ASCII 表格**与执行耗时；
- 内置点命令：
  - `.schema`：图模式（节点标签、关系类型、索引分布、节点/边计数）；
  - `.stats`：缓冲池命中率、物理 I/O、主文件与 WAL 体积、溢出（STEAL）次数；
  - `.checkpoint`：手动触发 WAL 落盘与截断；
  - `.dump <file>`：导出为可回灌的 Cypher 脚本（`-` 表示输出到 stdout）；
  - `.history`：查看最近命令历史；
  - `.help` / `.quit` / `.exit`。

### 13. 事务持久化与 ACID

- **WAL (Write-Ahead Log) 追加写入**：每条帧带有魔数 `GWAL`、长度及 CRC32 物理校验和；
- **崩溃自愈 (Crash Recovery)**：实例非正常退出重启时，自动按帧重放并安全截断损坏残缺半帧，只重放已提交事务；
- **事务失败原子回滚**：apply 阶段任一算子失败即整体回滚 —— 丢弃未提交页、按基线还原 WAL 位置索引、回拨内存元数据并使二级索引失效，失败事务零残留。

---

## 📂 项目结构

````text
graphlite-rs/
├── Cargo.toml                         # Workspace 根配置与 Rust Core 清单
├── README.md                          # 架构设计与技术文档
├── src/
│   ├── lib.rs                         # 核心 API 入口 (GraphLite, Transaction, dump_cypher)
│   ├── main.rs                        # 代码演示 Demo
│   ├── bin/
│   │   └── cli.rs                     # 交互式终端 REPL 客户端 (sqlite3 风格)
│   ├── page.rs                        # 4KB 物理页、NodeRecord (32B)、EdgeRecord (64B)、SlottedPropPage 定长编码
│   ├── buffer.rs                      # DiskManager 与 LRU BufferPoolManager (Pin/Unpin, Dirty, STEAL 溢出换页)
│   ├── disk_graph.rs                  # 磁盘直接寻址、双向双环免索引邻接链表、开槽属性页驱动
│   ├── index.rs                       # 标签倒排索引与属性二级索引系统
│   ├── cypher/                        # 原生 Cypher 字符串查询引擎
│   │   ├── mod.rs                     # 模块导出
│   │   ├── ast.rs                     # AST 节点定义 (Statement, Expr, Pattern, SetItem, OrderItem)
│   │   ├── lexer.rs                   # 词法分析器 (Tokenization, 行注释, 聚合关键字)
│   │   ├── parser.rs                  # 递归下降语法解析器
│   │   └── executor.rs                # 算子执行引擎与索引加速优化器
│   ├── graph.rs                       # 属性图领域模型与 Value 类型体系
│   ├── storage.rs                     # WalWriter 页级 WAL、CRC32 校验、Checkpoint 与崩溃恢复
│   ├── query.rs                       # 链式强类型 DSL 查询构造器
│   └── algo.rs                        # 内置图算法 (BFS、Dijkstra、环路、PageRank、WCC、K-Hop)
├── bindings/
│   ├── python/                        # 官方 Python SDK (PyO3 + Maturin)
│   │   ├── Cargo.toml
│   │   ├── pyproject.toml
│   │   ├── src/lib.rs                 # CPython 原生扩展模块导出
│   │   ├── tests/test_graphlite.py    # 端到端 Python 测试套件
│   │   └── README.md
│   └── nodejs/                        # 官方 Node.js / TypeScript SDK (NAPI-RS)
│       ├── Cargo.toml
│       ├── package.json
│       ├── build.rs
│       ├── src/lib.rs                 # Node-API 模块导出
│       ├── index.js                   # 原生模块智能加载胶水层
│       ├── index.d.ts                 # 完备 TypeScript 类型定义
│       ├── test.mjs                   # ESM 测试脚本
│       └── README.md
└── tests/
    ├── integration_tests.rs           # 核心 CRUD / ACID / 并发 / 索引 / 外存压测回归套件
    ├── cypher_advanced_tests.rs       # Cypher 1.0 语法闭环 (SET / DETACH DELETE / 排序分页 / 聚合)
    ├── analytics_tests.rs             # PageRank / 弱连通分量 / K-Hop 子图分析
    ├── steal_spill_tests.rs           # STEAL 溢出、回滚零污染、Checkpoint 语义
    ├── slotted_property_tests.rs      # 开槽页打包 / 槽位复用 / 页内压实 / 40k 密度指标
    ├── batch_tx_tests.rs              # 批量事务、单次 fsync 契约、批量吞吐
    ├── edge_locality_tests.rs         # 批量织网等价性 / 自环 / 假溢出消除 / 算法一致性
    ├── production_safety_tests.rs     # 排他锁 / 完整性校验 / 错误不静默 / 中毒恢复
    └── cli_tests.rs                   # 交互式 REPL 端到端 (多行输入 / 点命令 / dump 回灌)
├── benches/
│   ├── throughput.rs                # 可复现吞吐基准（每个场景自打印配置）
│   ├── pool_probe.rs                # 池容量 vs 吞吐关系（可切换分块数）
│   └── mem_probe.rs                 # 单事务内存占用剖析
└── ROADMAP.md                       # 后续规划与「引用性能数字的规则」
```

---

## 🚀 快速上手

### 1. 启动交互式终端 (CLI REPL)

```bash
cargo run --bin graphlite-cli -- mydb.db
````

在终端中执行语句并查看 ASCII 表格：

```text
============================================================
       GraphLite-RS Interactive Shell (SQLite 3.0 Edition)
       Connected to: mydb.db
       Enter '.help' for usage hints. Terminate queries with ';'.
============================================================
graphlite> CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32});
Query OK, Created 2 nodes, 1 relationships. (2.15ms)

graphlite> MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;
+---------+--------+-------+
| a.name  | b.name | b.age |
+---------+--------+-------+
| 'Alice' | 'Bob'  | 32    |
+---------+--------+-------+
1 row(s) in set (45.20µs)

graphlite> MATCH (p:Person)
    ...> RETURN p.name, p.age
    ...> ORDER BY p.age DESC SKIP 0 LIMIT 5;
+---------+-------+
| p.name  | p.age |
+---------+-------+
| 'Bob'   | 32    |
| 'Alice' | 28    |
+---------+-------+
2 row(s) in set (88.10µs)

graphlite> .schema

--- Graph Schema & Statistics ---
  Nodes:              2
  Edges:              1
  Labels (1):         Person
  Edge Types (1):     KNOWS
  Property Indices (2):
    - (Person).age
    - (Person).name
  Database File:      mydb.db

graphlite> .stats

--- 4KB Buffer Pool Metrics & Disk Stats ---
  Pool Capacity:      1024 frames (4096 KB)
  Used Frames:        2
  Dirty Frames:       1
  Cache Hits:         4
  Cache Misses:       2
  Cache Hit Rate:     66.67%
  Physical Reads:     0 pages
  Physical Writes:    0 pages
  Main File Size:     0 bytes (0.00 KB)
  WAL Size:           6132 bytes (5.99 KB)
  WAL Resident Pages: 2
  Spill (STEAL) Ops:  0

graphlite> .dump backup.cypher
Dumped 2 nodes and 1 edges to 'backup.cypher'.

graphlite> .checkpoint
Checkpoint completed in 1.45ms

graphlite> .quit
Bye!
```

---

### 2. Rust API 嵌入式调用

在项目的 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
graphlite-rs = "1.0.0-rc.1"
```

代码中使用：

```rust
use std::collections::{HashMap, HashSet};

use graphlite::{
    Direction, GraphLite, GraphError, Value,
    SMALL_POOL_FRAMES, DEFAULT_BUFFER_POOL_FRAMES, MEDIUM_POOL_FRAMES, LARGE_POOL_FRAMES,
};

fn main() -> Result<(), GraphError> {
    // 1. 打开数据库：推荐使用按 MB 指定内存的便捷构造器 (如 16MB 缓冲池)
    let db = GraphLite::open_with_pool_mb("mydb.db", 16)?;
    // 也可以使用命名常量或指定精准物理帧数：
    // let db = GraphLite::open_with_pool_size("mydb.db", DEFAULT_BUFFER_POOL_FRAMES)?; // 4MB / 1024 帧
    // let db = GraphLite::open_with_pool_size("mydb.db", LARGE_POOL_FRAMES)?; // 64MB / 16384 帧

    // 2. 原生 Cypher 变更
    db.execute("CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32})")?;

    // 3. 原生 Cypher 查询 (自动命中二级索引)
    let result = db.query_cypher("MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 30 RETURN a.name, b.name, b.age")?;
    for row in result.rows {
        println!("{:?}", row.values);
    }

    // 4. 显式批量事务：整个批次只触发一次 WAL fsync
    db.with_transaction(|tx| {
        for i in 0..100_000 {
            tx.add_node(
                HashSet::from(["Bulk".to_string()]),
                HashMap::from([("idx".to_string(), Value::from(i))]),
            )?;
        }
        Ok(())
    })?;

    // 5. 强类型算法接口 (Dijkstra 最短路径)
    if let Some((cost, path)) = db.dijkstra(1, 2, Some("KNOWS")) {
        println!("Shortest cost: {}, path: {:?}", cost, path);
    }

    // 6. 图算法：PageRank / 弱连通分量 / K-Hop 子图
    for score in db.pagerank_with(0.85, 100, 1e-6) {
        println!("node {} influence {}", score.node_id, score.score);
    }
    let components = db.weakly_connected_components();
    println!("{} weakly connected components", components.len());

    let sub = db.k_hop_subgraph_with(1, 2, Direction::Outgoing, Some("KNOWS"))?;
    println!("2-hop subgraph: {} nodes, {} edges", sub.nodes.len(), sub.edges.len());

    // 7. 导出为可回灌的 Cypher 脚本
    let mut dump: Vec<u8> = Vec::new();
    db.dump_cypher(&mut dump)?;

    // 8. 检查点持久化
    db.checkpoint()?;
    Ok(())
}
```

---

### 3. Python SDK 快速上手

安装：

```bash
pip install graphlite
# 或从源码开发安装：cd bindings/python && maturin develop
```

使用代码：

```python
import graphlite

# 1. 打开或创建数据库 (设置 Buffer Pool 帧数，默认 1024 = 4MB)
db = graphlite.GraphLite.open("mydb.db", pool_size=1024)

# 2. 执行 Cypher CREATE 语句
db.execute("CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32});")

# 3. 执行 Cypher 查询，自动转为 Python list[dict]
results = db.query("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;")
for row in results:
    print(row)  # {'a.name': 'Alice', 'b.name': 'Bob', 'b.age': 32}

# 4. 最短路径算法 (Dijkstra) / 无权 BFS / 环检测
res = db.dijkstra(1, 2, "KNOWS")
if res:
    cost, path = res
    print(f"Cost: {cost}, Path: {path}")
print(db.bfs(1, 2, "KNOWS"), db.has_cycle())

# 5. 图算法：PageRank / 弱连通分量 / K-Hop 子图
ranks = db.pagerank(0.85, 100, 1e-6)          # [(node_id, score), ...]
components = db.weakly_connected_components() # [[node_id, ...], ...]
sub = db.k_hop_subgraph(1, 2, "outgoing", "KNOWS")

# 6. Cypher 1.0 全语法：SET / DETACH DELETE / 排序分页 / 聚合
db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 31, a:Employee;")
db.query("MATCH (p:Person) RETURN count(p), avg(p.age) ORDER BY p.age DESC SKIP 1 LIMIT 5;")
db.execute("MATCH (n:Person {name: 'Bob'}) DETACH DELETE n;")

# 7. 显式批量事务：整批只触发一次 WAL fsync
with db.begin_transaction() as tx:
    for i in range(100_000):
        tx.add_node(["Bulk"], {"idx": i, "name": f"bulk-{i}"})

# 8. 图模式 introspect 与逻辑导出（可回灌的 Cypher 脚本）
print(db.labels(), db.edge_types())
open("backup.cypher", "w").write(db.dump_cypher())

# 9. 查看 4KB Buffer Pool 与 WAL 实时监控指标
print(db.stats())

# 11. 检查点刷盘
db.checkpoint()
```

---

### 4. Node.js / TypeScript SDK 快速上手

安装：

```bash
npm install graphlite-node
# 或 npm install @graphlite/core
```

TypeScript / JavaScript 使用：

```typescript
import { GraphLite } from "graphlite-node";

// 1. 打开数据库 (poolSize 为缓冲池帧数：1024 = 4MB)
const db = GraphLite.open("mydb.db", 1024);

// 2. 原生 Cypher 语句执行
db.execute(
  "CREATE (a:Person {name: 'Alice', age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: 'Bob', age: 32});",
);

// 3. 原生 Cypher 查询，返回结构化 JSON 对象数组
const results = db.query(
  "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;",
);
console.log(results);
// [ { 'a.name': 'Alice', 'b.name': 'Bob', 'b.age': 32 } ]

// 4. Dijkstra 最短路径 / 无权 BFS / 环检测
const sp = db.dijkstra(1, 2, "KNOWS");
if (sp) {
  console.log(`Cost: ${sp.cost}, Path: ${sp.path}`);
}
console.log(db.bfs(1, 2, "KNOWS"), db.hasCycle());

// 5. 图算法：PageRank / 弱连通分量 / K-Hop 子图
const ranks = db.pageRank(0.85, 100, 1e-6); // [{ node_id, score }, ...]
const components = db.weaklyConnectedComponents();
const sub = db.kHopSubgraph(1, 2, "outgoing", "KNOWS");

// 6. Cypher 1.0 全语法：SET / DETACH DELETE / 排序分页 / 聚合
db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 31, a:Employee;");
db.query(
  "MATCH (p:Person) RETURN count(p), avg(p.age) ORDER BY p.age DESC SKIP 1 LIMIT 5;",
);
db.execute("MATCH (n:Person {name: 'Bob'}) DETACH DELETE n;");

// 7. 显式批量事务：整批只触发一次 WAL fsync
const tx = db.beginTransaction();
for (let i = 0; i < 100000; i++) {
  tx.addNode(["Bulk"], { idx: i, name: `bulk-${i}` });
}
tx.commit();

// 8. 图模式 introspect 与逻辑导出（可回灌的 Cypher 脚本）
console.log(db.labels(), db.edgeTypes());
const dump = db.dumpCypher();

// 9. 查看 Buffer Pool 与 WAL 统计指标
console.log(db.stats());

// 10. Checkpoint 刷盘
db.checkpoint();
```

---

## 🧪 综合测试套件验证 (`cargo test`)

回归套件位于 `tests/integration_tests.rs`，覆盖 26 个严苛核心场景，包括：

1. **基础 CRUD 与属性更新测试**：验证各类属性类型动态转换、免索引邻接与级联删除；
2. **多跳社交图谱遍历测试**：2度好友推荐、方向控制与组合属性谓词过滤；
3. **带权 Dijkstra 最短路径与环路检测测试**：验证 Dijkstra 最优性与闭合环路提取准确性；
4. **事务原子性与 Rollback 测试**：显式回滚及 Drop 隐式回滚后的零状态残留；
5. **突发断电崩溃自愈测试**：未优雅退出直接 Drop 实例并在 WAL 尾部追加注入残损半帧，新实例 100% 完整复原；
6. **20 线程并发读写压力测试**：10个读线程与10个写线程高频并发执行，零死锁、零数据竞争；
7. **4KB 缓冲池受限大图压测 (核心跃升验证)**：Buffer Pool 限制为 2MB（512 Pages），单事务插入 20,000 个节点与 50,000 条边，高频执行多跳图遍历与 Dijkstra 最短路径，验证 STEAL 溢出换页下数据的绝对准确与内存硬约束；
8. **原生 Cypher 引擎端到端测试**：CREATE、MATCH、WHERE、RETURN、LIMIT、DETACH DELETE 全语法流程；
9. **二级索引加速有效性测试**：验证 Label Index 与 Property Index 的 $O(1)$ 点查与属性自适应更新；
10. **纯磁盘真外存压测 (1MB 极小内存限制)**：Buffer Pool 硬约束为 256 帧（1MB），单事务写入 10,000 个节点与 20,000 条边，执行纯磁盘 Cypher 查询与 Dijkstra，验证内存零暴涨与磁盘游标准确性。

在此之上，1.0 新增四个专项验证套件：

- **`cypher_advanced_tests.rs`（14 个用例）**：`SET` 属性/标签、`DETACH DELETE` 级联与 Freelist 槽位复用、`DELETE` 语义（含带边节点显式报错）、`ORDER BY` / `SKIP` / `LIMIT`、`count/sum/avg/min/max` 全局与分组聚合、变长多跳与无向边、多模式连接、`MATCH ... CREATE`、`RETURN *` 展开、变更持久化、**Cypher 脚本全量导出与回灌重放一致性**；
- **`analytics_tests.rs`（6 个用例）**：PageRank 星型/链式拓扑与分数归一、阻尼因子边界、WCC 多孤岛与环形图、K-Hop 子图方向与类型过滤，以及**在 1MB 受限缓冲池下**执行全部三类算法；
- **`steal_spill_tests.rs`（5 个用例）**：超大事务在 1MB/2MB 下成功且帧占用受控、溢出后回滚的主库字节级零污染、检查点将 WAL 落盘并清空索引、溢出后遍历与分析准确性、多标签跨溢出重启一致性；
- **`cli_tests.rs`（6 个用例）**：多行输入与续行提示符、ASCII 表格渲染、`.schema`/`.stats`/`.checkpoint`/`.help`、`.dump` 导出后**回灌到新库的数据一致性**、语法错误后会话存活。

1.1 再新增三个专项验证套件：

- **`slotted_property_tests.rs`（7 个用例）**：多条小属性共用一页（400 条记录不得占满 400 页）、删除后槽位空间复用（文件体积不得成倍膨胀）、随机删除 + 重插后页内压实内容无损、**1KB 内联/溢出边界两侧**（900B / 1200B / 11KB 与边属性同规格）、**40,000 实体密度断言（压缩 ≥15×）**、1MB 缓冲池下的密度与帧占用、属性反复更新与删除的存储卫生；
- **`batch_tx_tests.rs`（7 个用例）**：**单事务 10,000 次写入恰好触发一次 fsync**（对照逐条自动提交的 1:1 基线）、`:memory:` 批量吞吐达标（优化构建 20,000+ ops/s）、批量相对自动提交 >20×、批量回滚字节级零污染、`with_transaction` 闭包失败自动回滚、1MB 缓冲池下 10,000 节点 + 20,000 边批量事务（且恰好 2 次 fsync）、批量混合增删改的原子提交；
- **`edge_locality_tests.rs`（9 个用例）**：批量织网与逐条插入的**图结构完全等价**（含自环、重复边、扇入扇出）、自环不破坏出边链、**跨批次链头正确延续**、受限内存下**假溢出消除**（spill/边 < 0.1）、批量写入后 PageRank/WCC/K-Hop **结果逐位一致**、批量织网失败原子回滚零污染、多级页目录页常驻与深度寻址、**两种提交路径数学级算法等价断言（PageRank 漂移 < 1e-12 / WCC 拓扑完全一致）**、**混合事务安全路由防线**；
- **`production_safety_tests.rs`（12 个用例）**：第二个句柄被 `DatabaseLocked` 拒绝且释放后可重开、`:memory:` 不加锁、**真实子进程**验证跨进程互斥（并断言子进程确实跑了目标用例，避免"假通过"）、WAL 回放在持锁下进行、健康库通过校验（阴性对照）、破坏节点页被检出、**仅破坏链指针也被度数守恒 oracle 检出**、`try_get_node` 保留错误而 `get_node` 折叠为 `None`、中毒锁可恢复、**内部 panic 后引擎仍可服务**。

> **当前全量测试覆盖**：26 + 14 + 6 + 5 + 6 + 7 + 7 + 9 + 12 = **92 个测试用例，91 个通过、0 失败、1 个按设计 `#[ignore]`**（跨进程锁的子进程探针，由父测试显式拉起）。`cargo check` / `cargo clippy -D warnings` 均零告警。

### 运行全部测试

```bash
cargo test --workspace
```

### 单独运行某个专项套件

```bash
cargo test --test cypher_advanced_tests
cargo test --test analytics_tests
cargo test --test steal_spill_tests
cargo test --test slotted_property_tests
cargo test --test edge_locality_tests
cargo test --test production_safety_tests
cargo test --test cli_tests

# 吞吐验收请用优化构建（debug 构建会保留一个较低的功能性下限）
cargo test --release --test batch_tx_tests -- --nocapture
```

---

## 📊 性能基准（可复现）

仓库自带基准程序，**每个场景都会打印自己的配置**，因此数字无法脱离测量条件被引用。

```bash
# 完整规模（千万级节点，耗时数分钟且会写入数 GB 临时文件）
cargo bench --bench throughput

# 冒烟规模（快速验证基准本身可运行）
GL_SCALE=small cargo bench --bench throughput

# 定位「池容量 vs 吞吐」关系，并把工作负载拆成多个事务
cargo bench --bench pool_probe
GL_NODES=1000000 GL_EDGES=4000000 GL_CHUNKS=40 cargo bench --bench pool_probe

# 单事务内存占用剖析
cargo bench --bench mem_probe
```

SDK 侧基准：

```bash
cd bindings/python && PYTHONPATH=. python3 benches/throughput.py
cd bindings/nodejs && node benches/throughput.mjs
```

SDK 实测（同一台 M4，冒烟规模 10 万节点 + 40 万边）：

| SDK            | 节点写入     | 边写入        |
| -------------- | ------------ | ------------- |
| Python (PyO3)  | 63,000 ops/s | 112,000 ops/s |
| Node.js (NAPI) | 64,000 ops/s | 117,000 ops/s |

> 差距来自**跨语言 FFI 边界**：SDK 每条写入都是一次跨语言调用，
> 而 Rust 原生批量路径在语言内部完成。这是接口形态的固有成本，不是引擎慢。
> 若 SDK 侧追求吞吐，应改用批量传入（例如一次提交一个数组），
> 这已记录在 ROADMAP 中。

**引用性能数字的规则**：必须同时给出上述命令、硬件（CPU / 存储 / OS）与属性载荷。
本仓库曾出现过无法复现的数字，因此该规则是硬性要求，详见 [ROADMAP.md](ROADMAP.md)。

---

## 📜 许可证

本项目**双授权**，你可以任选其一：

1. **GNU Affero General Public License v3.0**（`AGPL-3.0-only`）—— 免费，见 [LICENSE](LICENSE)；
2. **商业许可** —— 用于 AGPL 不适用的场景。联系方式：**luhuizhx@gmail.com**。

### AGPL 限制的不是"赚钱"，而是"闭源"

| 用法                             | AGPL 够用吗                                                       |
| -------------------------------- | ----------------------------------------------------------------- |
| 公司内部使用、个人项目、学术用途 | ✅ 免费，随便用                                                   |
| 不改代码，直接对外提供服务       | ✅ 免费                                                           |
| 卖托管、卖技术支持               | ✅ 免费                                                           |
| **改了代码后对外提供服务**       | ⚠️ 免费，但 AGPL 第 13 条要求你向使用者提供**你改动后的完整源码** |
| **打包进闭源产品分发**           | ❌ 需要商业许可                                                   |
| 基于它做开源产品                 | ✅ 只要你的产品与 AGPL 兼容                                       |

一句话：触发义务的是「**修改 + 对外提供服务**」或「**分发**」，不是「赚钱」。

### 什么时候需要商业许可

- 想嵌入**闭源**产品并分发给客户；
- 想以**修改后**的版本提供网络服务，但不公开改动；
- 所在组织的政策禁止使用 AGPL 依赖；
- 需要质保、赔偿条款或技术支持承诺。

需要商业许可请邮件至 **luhuizhx@gmail.com**，说明：公司、用途、是否会修改、是分发还是仅对外提供服务。

### 贡献

双授权模式成立的前提是**单一主体持有全部版权**，因此外部贡献需签署
[贡献者许可协议 (CLA)](CLA.md)——你保留版权，仅授予本项目同时以 AGPL 与商业条款进行许可的权利。
详见 [LICENSING.md](LICENSING.md) 与 [CONTRIBUTING.md](CONTRIBUTING.md)。

> 以上为简明说明，不构成法律建议；AGPL 的正式条款以 [LICENSE](LICENSE) 为准。
