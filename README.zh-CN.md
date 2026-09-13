# NervusDB

[English](README.md) · **中文**

[![CI](https://github.com/ysankpia/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/ysankpia/nervusdb/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.89%2B-orange.svg)](https://www.rust-lang.org)

嵌入式、单文件**属性图数据库**：把 SQLite 的模式用在图上。磁盘上只有两个文件，没有服务端，
没有守护进程；常驻内存由可配置的缓冲池决定，而不是由数据规模决定。

```rust
use nervusdb::{NervusDb, GraphError};

let db = NervusDb::open("novel.db")?;

db.with_transaction(|tx| {
    let lin = tx.add_node(
        std::collections::HashSet::from(["Character".to_string()]),
        std::collections::HashMap::from([("name".to_string(), "林渊".into())]),
    )?;
    let su = tx.add_node(
        std::collections::HashSet::from(["Character".to_string()]),
        std::collections::HashMap::from([("name".to_string(), "苏晴".into())]),
    );
    tx.add_edge(lin, su, "KNOWS", std::collections::HashMap::new(), 1.0)?;
    Ok(())
})?;

let rows = db.query_cypher(
    "MATCH (a:Character)-[:KNOWS]->(b) RETURN a.name AS a, b.name AS b",
)?;
```

## 状态

**`v0.1.0` —— 首次以本名发布。** 引擎、Cypher 查询面、图算法与安全保证均已实现，
由 198 个通过的测试覆盖（共 199 个，其中 1 个刻意 `#[ignore]`）。磁盘格式冻结在
**版本 5**；关于该冻结承诺的唯一一次例外（随项目改名的 Page 0 魔数）及其迁移路径，
见 [`FORMAT.md`](FORMAT.md)。

若干已知缺口仍在，投入生产前请先读
[已知限制](ROADMAP.md#next-planned)。

## 特性

- **嵌入式、单文件。** `{path}` 加一个页级 WAL `{path}.wal`。无附属文件，无外部服务。
- **内存有界。** 4KB 页缓冲池，O(1) LRU 淘汰。常驻内存由池大小决定而非数据集大小：
  已在 4.34 GB 的图上（6890 万条边）用 1 GiB 池验证，超出部分由构造保证有界。
- **磁盘原生邻接。** 定长 32 字节节点记录与 64 字节边记录，O(1) 物理寻址，双环边指针链。
  找邻居从不做全表扫描。
- **紧凑属性存储。** 开槽页在一页内打包多条载荷；4 万条约 150 字节的实体占 7.9MB。
- **ACID。** 显式事务、单次 fsync 的组提交、池放不下时的 STEAL 溢出到 WAL、
  崩溃恢复、主文件逐字节复原的精确回滚。
- **Cypher。** `CREATE`、`MATCH`（多模式）、`MERGE`（幂等写）、`UNWIND`（一条语句批量入库）、
  `WHERE`、`SET`、`DELETE` / `DETACH DELETE`、`ORDER BY`、`SKIP`、`LIMIT`、聚合、
  变长路径、`EXPLAIN`。

  ```cypher
  UNWIND [1, 2, 3] AS i CREATE (n:Num {v: i})   -- 一条语句建三个节点
  MERGE (u:User {name: 'alice'})                -- 先创建，之后复用
  ```

- **自洽读。** `NervusDb::read_snapshot()` 让一次多步遍历看到的始终是同一个状态，
  因此「先读节点、再读它的边」不会撞上中间被并发删掉的那条边。快照存活期间会阻塞写者；
  要真正的读写并行需要页级版本可见性（[ROADMAP](ROADMAP.md) 第 1 项）。
- **图算法。** BFS、Dijkstra、环检测、PageRank、弱连通分量、K 跳子图 —— 全部走磁盘游标。
- **生产安全。** 排他打开锁、结构化完整性校验、保留错误的读访问器。
- **SDK。** Python（PyO3）与 Node.js（NAPI-RS），支持事务、批量写与逻辑导出。
  库就是接口：没有需要额外同步的独立 CLI 或 GUI。

## 文档

**[docs/index.md](docs/index.md) 列出了全部文档以及各自该在什么时候读。**
开始之前值得知道的三份：

| 文档                                         | 什么时候读                             |
| -------------------------------------------- | -------------------------------------- |
| [FORMAT.md](FORMAT.md)                       | 需要精确字节，或需要冻结格式的契约时。 |
| [docs/architecture.md](docs/architecture.md) | 想知道**为什么**是这样设计时。         |
| [AGENTS.md](AGENTS.md)                       | **改代码之前。** 不变量与验证流程。    |

**深度文档目前只有英文版**，且英文版是唯一权威来源。本文件刻意不复述它们的内容——
同一件事写两遍必然漂移，而这个小仓库刚刚才因为文档漂移吃过亏（见 CHANGELOG）。

## 安装

```toml
[dependencies]
nervusdb = "0.1.0"
```

Python 与 Node.js 的 SDK **尚未发布到 PyPI / npm**。目前需从源码构建，
见 [bindings/](bindings/)。

## 快速上手

### Python

```python
import nervusdb

db = nervusdb.NervusDb.open("novel.db")
with db.begin_transaction() as tx:
    lin = tx.add_node(["Character"], {"name": "林渊"})
    su = tx.add_node(["Character"], {"name": "苏晴"})
    tx.add_edge(lin, su, "KNOWS", {"since": 2020}, 1.0)

rows = db.query_cypher(
    "MATCH (a:Character)-[:KNOWS]->(b) RETURN a.name AS a, b.name AS b"
)
print(rows)

db.dump_cypher("backup.cypher")   # 逻辑导出，可回灌到新库
db.backup("snapshot.db")          # 一致的在线副本
```

### Node.js

```javascript
import { NervusDb } from "nervusdb";

const db = NervusDb.open("novel.db");
const tx = db.beginTransaction();
const lin = tx.addNode(["Character"], { name: "林渊" });
const su = tx.addNode(["Character"], { name: "苏晴" });
tx.addEdge(lin, su, "KNOWS", { since: 2020 }, 1.0);
tx.commit();
```

### 选择内存预算

```rust
use nervusdb::{NervusDb, SMALL_POOL_FRAMES, DEFAULT_BUFFER_POOL_FRAMES, LARGE_POOL_FRAMES};

let db = NervusDb::open_with_pool_mb("mydb.db", 16)?;                     // 16 MB
let db = NervusDb::open_with_pool_size("mydb.db", SMALL_POOL_FRAMES)?;    // 1 MB
let db = NervusDb::open("mydb.db")?;   // 默认 4 MB（DEFAULT_BUFFER_POOL_FRAMES）
```

### 批量写入

把批量导入包在**一个**事务里。整批只付一次 `fsync`，而不是每条一次；
写路径也能成批织入边链，而不是逐条跳页：

```rust
db.with_transaction(|tx| {
    for i in 0..100_000 {
        tx.add_node(
            HashSet::from(["Bulk".to_string()]),
            HashMap::from([("idx".to_string(), Value::from(i))]),
        )?;
    }
    Ok(())
})?;
```

**超大批次要分块。** 一个事务会把全部动作留在内存里；实测每个节点动作约 502 字节、
每条边约 128 字节，因此默认上限为 400 万动作（约 812 MB）。超过会**明确报错**而不是
静默分块——分块等于提交事务的一部分，会破坏「要么全做要么全不做」的回滚保证。
测量数据见 [docs/benchmarks.md](docs/benchmarks.md)。

### 完整性与错误处理

`get_node` / `get_edge` 是**有损**的（把存储错误折叠成 `None`）。
生产代码请用 `try_get_node` / `try_get_edge`，它们保留错误，
`Ok(None)` 只表示「确实不存在」。这一区别由测试固定，见
[docs/testing.md](docs/testing.md)。

## 项目结构

```text
src/        核心库（零运行时依赖）
bindings/   面向 Python 与 Node.js 的 FFI 层
tests/      15 个套件、177 个用例，另有 20 个内联单元测试与 2 个 doctest
benches/    可复现的吞吐、池大小与内存探针
docs/       深度文档（英文）
```

## 开发

分支只有两个：`develop` 上开发，`main` 是项目本体。改完跑完整门禁：

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

细节见 [AGENTS.md §3](AGENTS.md) 与 [docs/testing.md](docs/testing.md)。

## 许可

AGPL-3.0，另有商业许可。见 [LICENSING.md](LICENSING.md)。
