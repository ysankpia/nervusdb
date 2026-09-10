# GraphLite-RS 1.1 (图数据库界的 SQLite)

> **GraphLite-RS** 是一个采用现代 Rust 构建的工业级、嵌入式、单文件持久化属性图数据库引擎。
> 它融合了 **SQLite 的轻量零依赖与单文件部署体验** 与 **原生图数据库（如 Neo4j）的定长记录磁盘免索引邻接性能**，彻底打破超大图谱常驻内存瓶颈，全面对齐关系型数据库界的 SQLite。

## 1.1 关键跃升（实测数据）

| 指标                                  | 1.0                               | 1.1                              |
| ------------------------------------- | --------------------------------- | -------------------------------- |
| 40,000 实体落盘体积（属性净长 ~150B） | 162 MB（每实体独占一整张 4KB 页） | **7.90 MB**（压缩 **19.8×**）    |
| 每实体均摊体积                        | ~4096 B                           | **207 B**                        |
| 批量写入吞吐                          | ~180 ops/s（逐条自动提交）        | **11.7 万 ~ 20.3 万 ops/s**      |
| 单事务 WAL fsync 次数                 | 每条记录 1 次                     | **整个事务恰好 1 次**            |
| 属性记录框架开销                      | ~28 B/实体（bincode HashMap）     | **~6 B/实体**（varint 紧凑编码） |

> 实测口径：10,000 节点 + 30,000 边，节点与边各带一条 ~150 字节字符串属性；
> 「旧基线」= 实体数 × 4096B，即 1.0 的「每实体整页」布局。

### 边写入局部性（1.1 补强）

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

## 🌟 核心工业级架构亮点

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

物理格式版本 `DB_PAGE_VERSION = 2`（开槽属性页）。1.0 的「每实体整页」布局与 1.1 **不兼容**，打开旧文件会返回明确错误而非静默误读：

```bash
# 1.0 环境导出逻辑数据
graphlite-cli old.db
graphlite> .dump old_graph.cypher

# 1.1 环境导入到全新数据库
graphlite-cli new.db < old_graph.cypher
```

`.dump` 以 `CREATE` + `SET` 语句导出，回灌到既有节点时是**替换**语义，因此天然幂等。

### 8. 原生 Cypher 字符串查询引擎 (对齐 SQLite SQL 能力)

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

### 9. 企业级图算法引擎 (Graph Analytics)

- **BFS 最短路径 / Dijkstra 带权最短路**：纯磁盘邻接流式遍历；
- **PageRank 阻尼迭代**：可配置 `damping_factor` / `max_iterations` / `tolerance`，支持悬挂节点质量再分配，分数归一为 1.0；
- **弱连通分量 (WCC)**：并查集划分社群与孤岛检测，按分量规模降序返回；
- **K-Hop 局部子图提取**：抽取指定节点 K 步内的节点与边结构，支持方向与关系类型过滤；
- **有向环路检测**：三色标记法，支持全图环路枚举。

### 10. 属性与标签二级索引加速 (Secondary Indexing)

- **标签索引 (Label Index)**：`Label -> BTreeSet<NodeId>` 倒排集合；
- **属性索引 (Property Index)**：`(Label, PropKey) -> BTreeMap<Value, BTreeSet<NodeId>>`；
- **索引感知查询优化器**：执行包含标签或等值属性定位的查询时，自动优先命中二级索引获取起始点候选集，**严禁全图扫表**；
- **索引自愈**：属性更新 / 标签追加 / 事务失败后索引自动保持一致，冷重启后按需重建，杜绝陈旧索引幻读。

### 11. 交互式终端 REPL 客户端 (媲美 sqlite3 CLI)

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

### 12. 事务持久化与 ACID

- **WAL (Write-Ahead Log) 追加写入**：每条帧带有魔数 `GWAL`、长度及 CRC32 物理校验和；
- **崩溃自愈 (Crash Recovery)**：实例非正常退出重启时，自动按帧重放并安全截断损坏残缺半帧，只重放已提交事务；
- **事务失败原子回滚**：apply 阶段任一算子失败即整体回滚 —— 丢弃未提交页、按基线还原 WAL 位置索引、回拨内存元数据并使二级索引失效，失败事务零残留。

---

## 📂 项目结构

```text
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
    └── cli_tests.rs                   # 交互式 REPL 端到端 (多行输入 / 点命令 / dump 回灌)
```

---

## 🚀 快速上手

### 1. 启动交互式终端 (CLI REPL)

```bash
cargo run --bin graphlite-cli -- mydb.db
```

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
graphlite-rs = "0.1.0"
```

代码中使用：

```rust
use std::collections::{HashMap, HashSet};

use graphlite::{Direction, GraphLite, GraphError, Value};

fn main() -> Result<(), GraphError> {
    // 1. 打开数据库 (支持配置 Buffer Pool 大小，如 512 帧 = 2MB)
    let db = GraphLite::open_with_pool_size("mydb.db", 1024)?;

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

# 7. Cypher 1.0 全语法：SET / DETACH DELETE / 排序分页 / 聚合
db.execute("MATCH (a:Person {name: 'Alice'}) SET a.age = 31, a:Employee;")
db.query("MATCH (p:Person) RETURN count(p), avg(p.age) ORDER BY p.age DESC SKIP 1 LIMIT 5;")
db.execute("MATCH (n:Person {name: 'Bob'}) DETACH DELETE n;")

# 8. 显式批量事务：整批只触发一次 WAL fsync
with db.begin_transaction() as tx:
    for i in range(100_000):
        tx.add_node(["Bulk"], {"idx": i, "name": f"bulk-{i}"})

# 9. 图模式 introspect 与逻辑导出（可回灌的 Cypher 脚本）
print(db.labels(), db.edge_types())
open("backup.cypher", "w").write(db.dump_cypher())

# 10. 查看 4KB Buffer Pool 与 WAL 实时监控指标
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

- **`cypher_advanced_tests.rs`（13 个用例）**：`SET` 属性/标签、`DETACH DELETE` 级联与 Freelist 槽位复用、`DELETE` 语义（含带边节点显式报错）、`ORDER BY` / `SKIP` / `LIMIT`、`count/sum/avg/min/max` 全局与分组聚合、变长多跳与无向边、多模式连接、`MATCH ... CREATE`、`RETURN *` 展开、变更持久化；
- **`analytics_tests.rs`（6 个用例）**：PageRank 星型/链式拓扑与分数归一、阻尼因子边界、WCC 多孤岛与环形图、K-Hop 子图方向与类型过滤，以及**在 1MB 受限缓冲池下**执行全部三类算法；
- **`steal_spill_tests.rs`（5 个用例）**：超大事务在 1MB/2MB 下成功且帧占用受控、溢出后回滚的主库字节级零污染、检查点将 WAL 落盘并清空索引、溢出后遍历与分析准确性、多标签跨溢出重启一致性；
- **`cli_tests.rs`（6 个用例）**：多行输入与续行提示符、ASCII 表格渲染、`.schema`/`.stats`/`.checkpoint`/`.help`、`.dump` 导出后**回灌到新库的数据一致性**、语法错误后会话存活。

1.1 再新增三个专项验证套件：

- **`slotted_property_tests.rs`（7 个用例）**：多条小属性共用一页（400 条记录不得占满 400 页）、删除后槽位空间复用（文件体积不得成倍膨胀）、随机删除 + 重插后页内压实内容无损、**1KB 内联/溢出边界两侧**（900B / 1200B / 11KB 与边属性同规格）、**40,000 实体密度断言（压缩 ≥15×）**、1MB 缓冲池下的密度与帧占用、属性反复更新与删除的存储卫生；
- **`batch_tx_tests.rs`（7 个用例）**：**单事务 10,000 次写入恰好触发一次 fsync**（对照逐条自动提交的 1:1 基线）、`:memory:` 批量吞吐达标（优化构建 20,000+ ops/s）、批量相对自动提交 >20×、批量回滚字节级零污染、`with_transaction` 闭包失败自动回滚、1MB 缓冲池下 10,000 节点 + 20,000 边批量事务（且恰好 2 次 fsync）、批量混合增删改的原子提交；
- **`edge_locality_tests.rs`（7 个用例）**：批量织网与逐条插入的**图结构完全等价**（含自环、重复边、扇入扇出）、自环不破坏出边链、**跨批次链头正确延续**、受限内存下**假溢出消除**（spill/边 < 0.1）、批量写入后 PageRank/WCC/K-Hop **结果逐位一致**、批量织网失败原子回滚零污染、多级页目录页常驻与深度寻址。

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
cargo test --test cli_tests

# 吞吐验收请用优化构建（debug 构建会保留一个较低的功能性下限）
cargo test --release --test batch_tx_tests -- --nocapture
```

---

## 📜 许可证

本项目采用 **MIT 或 Apache-2.0** 双重开源许可证。
