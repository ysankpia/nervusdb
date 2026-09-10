# GraphLite-RS (图数据库界的 SQLite 3.0)

> **GraphLite-RS** 是一个采用现代 Rust 构建的工业级、嵌入式、单文件持久化属性图数据库引擎。
> 它融合了 **SQLite 的轻量零依赖与单文件部署体验** 与 **原生图数据库（如 Neo4j）的定长记录磁盘免索引邻接性能**，彻底打破超大图谱常驻内存瓶颈，全面对齐关系型数据库界的 SQLite 3.0。

---

## 🌟 核心工业级架构亮点

### 1. 4KB 磁盘分页与 Buffer Pool 换页引擎 (彻底破除 RAM 瓶颈)

- **物理分页存储**：数据文件按 4096 字节（4KB）划分，支持物理 Paged I/O。
- **BufferPoolManager 缓冲池**：
  - 内存占用严格受控在可配置阈值内（默认 1024 帧 = 4MB，测试时可调至 512 帧 = 2MB）；
  - 采用标准 **LRU (Least Recently Used)** 换页置换淘汰算法；
  - 严谨的 **Page Pin/Unpin 引用计数** 与 **Dirty 脏页标记**，按需主动/惰性刷盘；
  - 任何图操作均按需通过 Buffer Pool 调取对应磁盘页，常驻内存永远受控在设定阈值之内，即使面对上百 GB 乃至数百 GB 的大图也能稳定运行。

### 2. 定长紧凑记录与磁盘免索引邻接 (Fixed-Size Record & Disk Adjacency)

- **$O(1)$ 磁盘物理直接寻址**（无需任何二级页表）：
  - **NodeRecord（定长 32 字节）**：`in_use (1B)` + `reserved (3B)` + `label_id (4B)` + `first_outgoing_edge_id (8B)` + `first_incoming_edge_id (8B)` + `prop_page_id (4B)` + `inline_prop_val (4B)`。每页精准容纳 128 条记录。
  - **EdgeRecord（定长 64 字节）**：`in_use (1B)` + `reserved (3B)` + `edge_type_id (4B)` + `prop_page_id (4B)` + `reserved2 (4B)` + `src_id (8B)` + `dst_id (8B)` + `weight (8B)` + `src_prev_edge_id (8B)` + `src_next_edge_id (8B)` + `dst_next_edge_id (8B)`。每页精准容纳 64 条记录。
  - **直接寻址算法**：
    $$\text{PageId} = \text{BasePage} + \frac{N \times \text{sizeof(Record)}}{4096}$$
    $$\text{Offset} = (N \times \text{sizeof(Record)}) \pmod{4096}$$
- **Freelist 空闲槽位复用**：在文件头 Page 0 维护 `first_free_node_id` 与 `first_free_edge_id`，删除记录时将其作为链表节点串入空闲链表，新增记录时优先弹出空闲槽位复用，杜绝磁盘稀疏空洞。
- **磁盘双向双环链表邻接**：每条边与其源节点的出边和目的节点的入边链表相连，图遍历顺着磁盘指针逐页载入，杜绝全表扫描。
- **属性溢出页 (Property Overflow Pages)**：超出内联尺寸的字符串、多标签与复合属性存储在链式溢出页中。

### 3. 统一单文件物理存储与页级 WAL (Page-Level WAL)

- **单数据文件架构**：彻底切除任何内存图双写与多文件分散，统一为主数据文件 `{path}`（纯 4KB 物理分页数据文件）与预写日志 `{path}.wal`。
- **页级 WAL 记录**：WAL 日志记录标准物理页修改帧 `WalRecord::PageWrite { tx_id, page_id, data: 4KB, crc32 }`，事务提交时只落盘 WAL，Checkpoint 时将脏页同步回写至主文件并截断 WAL，崩溃自愈通过直接重放页物理数据无损恢复。
- **并发粒度下沉为页级门闩 (Page-Level Latches)**：彻底移除顶层针对整个图的粗暴独占大锁，Buffer Pool 中每个 Frame 享有独立读写保护，支持多线程并发遍历不同磁盘页。

### 3. 原生 Cypher 字符串查询引擎 (对齐 SQLite SQL 能力)

完整的词法（Lexer）、语法（Parser）与执行算子（Executor），支持直接执行标准 Cypher 字符串：

- **CREATE 变更**：
  ```cypher
  CREATE (a:Person {name: "Alice", age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: "Bob", age: 32})
  ```
- **MATCH ... WHERE ... RETURN ... LIMIT 查询**：
  ```cypher
  MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE b.age > 20 RETURN a.name, b.name, b.age LIMIT 10
  ```
- **多跳变长路径查询**：
  ```cypher
  MATCH (a)-[:KNOWS*1..3]->(b)
  ```
- **DETACH DELETE 级联删除**：
  ```cypher
  MATCH (n:Person {name: "Alice"}) DETACH DELETE n
  ```

### 4. 属性与标签二级索引加速 (Secondary Indexing)

- **标签索引 (Label Index)**：`Label -> BTreeSet<NodeId>` 倒排集合；
- **属性索引 (Property Index)**：`(Label, PropKey) -> BTreeMap<Value, BTreeSet<NodeId>>`；
- **索引感知查询优化器**：执行包含标签或等值属性定位的查询时，自动优先命中二级索引获取起始点候选集，**严禁全图扫表**。

### 5. 交互式终端 REPL 客户端 (媲美 sqlite3 CLI)

- 启动：`cargo run --bin graphlite-cli -- mydb.db`
- 交互式提示符 `graphlite> `，支持多行输入与分号终止；
- 打印对齐美观的 **ASCII 表格**；
- 支持内置点命令：`.schema`、`.stats`、`.checkpoint`、`.help`、`.quit`。

### 6. 事务持久化与 ACID

- **WAL (Write-Ahead Log) 追加写入**：每条帧带有魔数 `GWAL`、长度及 CRC32 物理校验和；
- **崩溃自愈 (Crash Recovery)**：实例非正常退出重启时，自动按帧重放并安全截断损坏残缺半帧，只重放已提交事务。

---

## 📂 项目结构

```text
/Users/luhui/Desktop/graphlite-rs/
├── Cargo.toml                         # Workspace 根配置与 Rust Core 清单
├── README.md                          # 架构设计与技术文档
├── src/
│   ├── lib.rs                         # 核心 API 入口 (GraphLite, Transaction)
│   ├── main.rs                        # 代码演示 Demo
│   ├── bin/
│   │   └── cli.rs                     # 交互式终端 REPL 客户端 (sqlite3 风格)
│   ├── page.rs                        # 4KB 物理页、NodeRecord (32B)、EdgeRecord (64B) 定长编码
│   ├── buffer.rs                      # DiskManager 与 LRU BufferPoolManager (Pin/Unpin, Dirty, Eviction)
│   ├── disk_graph.rs                  # 磁盘直接寻址、双向双环免索引邻接链表、属性溢出页驱动
│   ├── index.rs                       # 标签倒排索引与属性二级索引系统
│   ├── cypher/                        # 原生 Cypher 字符串查询引擎
│   │   ├── mod.rs                     # 模块导出
│   │   ├── ast.rs                     # AST 节点定义 (Statement, Expr, Pattern)
│   │   ├── lexer.rs                   # 词法分析器 (Tokenization)
│   │   ├── parser.rs                  # 递归下降语法解析器
│   │   └── executor.rs                # 算子执行引擎与索引加速优化器
│   ├── graph.rs                       # 内存属性图拓扑模型与 Value 类型体系
│   ├── storage.rs                     # WAL 预写日志与单文件快照管理
│   ├── query.rs                       # 链式强类型 DSL 查询构造器
│   └── algo.rs                        # 内置图算法 (BFS、Dijkstra、有向环路检测)
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
    └── integration_tests.rs           # 9大严苛综合集成测试套件
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
graphlite> CREATE (a:Person {name: "Alice", age: 28})-[:KNOWS {weight: 1.5}]->(b:Person {name: "Bob", age: 32});
Query OK, Created 2 nodes, 1 relationships. (2.15ms)

graphlite> MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name, b.age;
+---------+--------+-------+
| a.name  | b.name | b.age |
+---------+--------+-------+
| "Alice" | "Bob"  | 32    |
+---------+--------+-------+
1 row(s) in set (45.20µs)

graphlite> .stats

--- 4KB Buffer Pool Metrics & Disk Stats ---
  Pool Capacity:     1024 frames (4096 KB)
  Used Frames:       2
  Dirty Frames:      1
  Cache Hits:        4
  Cache Misses:      2
  Cache Hit Rate:    66.67%
  Physical Reads:    0 pages
  Physical Writes:   0 pages
  File Disk Size:    0 bytes (0.00 KB)

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
use graphlite::{GraphLite, GraphError, Value};

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

    // 4. 强类型算法接口 (Dijkstra 最短路径)
    if let Some((cost, path)) = db.dijkstra(1, 2, Some("KNOWS")) {
        println!("Shortest cost: {}, path: {:?}", cost, path);
    }

    // 5. 检查点持久化
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

# 4. 最短路径算法 (Dijkstra)
res = db.dijkstra(1, 2, "KNOWS")
if res:
    cost, path = res
    print(f"Cost: {cost}, Path: {path}")

# 5. 查看 4KB Buffer Pool 实时监控指标
print(db.stats())

# 6. 检查点刷盘
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

// 4. Dijkstra 最短路径算法
const sp = db.dijkstra(1, 2, "KNOWS");
if (sp) {
  console.log(`Cost: ${sp.cost}, Path: ${sp.path}`);
}

// 5. 查看 Buffer Pool 统计指标
console.log(db.stats());

// 6. Checkpoint 刷盘
db.checkpoint();
```

---

## 🧪 综合测试套件验证 (`cargo test`)

测试套件位于 `tests/integration_tests.rs`，覆盖 10 大严苛核心场景：

1. **基础 CRUD 与属性更新测试**：验证各类属性类型动态转换、免索引邻接与级联删除；
2. **多跳社交图谱遍历测试**：2度好友推荐、方向控制与组合属性谓词过滤；
3. **带权 Dijkstra 最短路径与环路检测测试**：验证 Dijkstra 最优性与闭合环路提取准确性；
4. **事务原子性与 Rollback 测试**：显式回滚及 Drop 隐式回滚后的零状态残留；
5. **突发断电崩溃自愈测试**：未优雅退出直接 Drop 实例并在 WAL 尾部追加注入残损半帧，新实例 100% 完整复原；
6. **20 线程并发读写压力测试**：10个读线程与10个写线程高频并发执行，零死锁、零数据竞争；
7. **4KB 缓冲池受限大图压测 (核心跃升验证)**：Buffer Pool 限制为 2MB（512 Pages），插入 20,000 个节点与 50,000 条边，高频执行多跳图遍历与 Dijkstra 最短路径，验证频繁换页（Eviction）下数据的绝对准确与零内存泄漏；
8. **原生 Cypher 引擎端到端测试**：CREATE、MATCH、WHERE、RETURN、LIMIT、DETACH DELETE 全语法流程；
9. **二级索引加速有效性测试**：验证 Label Index 与 Property Index 的 $O(1)$ 点查与属性自适应更新；
10. **纯磁盘真外存压测 (1MB 极小内存限制)**：Buffer Pool 硬约束为 256 帧（1MB），写入 10,000 个节点与 20,000 条边，执行纯磁盘 Cypher 查询与 Dijkstra，验证内存零暴涨与磁盘游标准确性。

### 运行全部测试

```bash
cd /Users/luhui/Desktop/graphlite-rs
cargo test
```

---

## 📜 许可证

本项目采用 **MIT 或 Apache-2.0** 双重开源许可证。
