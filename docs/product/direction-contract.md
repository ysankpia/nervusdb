# Direction Contract

## Product Definition

NervusDB is SQLite for property graphs: a Rust-first embedded graph database with
local file storage, WAL-backed crash recovery, persistent graph data, and a
small query surface.

## Primary User

The 0.1 user is a Rust application developer who needs embedded graph
persistence for local-first tools, dependency analysis, knowledge graphs,
ownership graphs, module graphs, or small relationship-heavy features.

## North Star Workflow

```
open(path) -> write graph data -> query one-hop/two-hop relationships -> crash/reopen -> trust results
```

## In Scope (0.1)

- **Rust embedded API** — `Db::open`, `WriteTxn`, `DbSnapshot`, `ReadTxn`, Mini-Cypher.
- **Local `.ndb` + `.wal` files** — 每数据库一个文件对。
- **WAL-backed crash recovery** — 提交的数据在 kill/reopen 后仍可读。
- **节点 / 边 / 标签 / 属性持久化** — 创建、读取、删除。
- **一个写入者 + 快照读取** — `begin_write` 串行化，读不受写阻塞。
- **标签扫描** — `MATCH (n:Label)`。
- **按关系类型遍历邻居** — `MATCH (a)-[:TYPE]->(b)`（一跳）、两跳。
- **Mini-Cypher** — 仅 `RETURN`/`MATCH`/`CREATE`/`SET`/`DELETE`/`LIMIT`/`EXPLAIN`。
- **CLI** — 本地调试、查询、写入、文件导入。
- **10 个可运行的 0.1 示例** — Social/Dependency/Knowledge Graph 等。

## Explicitly Deleted (Not Just Out Of Scope)

以下代码将从仓库中物理删除。Git 历史中有备份。

- **HNSW/向量搜索** — `nervusdb-storage/src/index/hnsw/` 全部（824 行）。删 `ordered-float`、`rand` 依赖。
- **Python 绑定** — `nervusdb-pyo3/` 全部。
- **Node.js 绑定** — `nervusdb-node/` 全部。
- **C API 绑定** — `nervusdb-capi/` 全部。
- **完整 openCypher 语法** — MERGE、UNWIND、CALL/子查询/存储过程、WITH、UNION、FOREACH、OPTIONAL MATCH、ORDER BY、SKIP、DISTINCT、聚合、CASE、EXISTS、模式推导、命名路径、变长路径、字符串高级函数、类型交替。
- **历史集成测试** — ~35 个 `tXXX_*.rs` 文件。
- **TCK 夹具** — `tck_harness.rs` + `opencypher_tck/`。
- **绑定测试** — `examples-test/` 全部。
- **CI 夜间工作流** — 10 个非 `ci.yml` 的 workflow。
- **历史验证脚本** — 31 个非核心的脚本。
- **fuzz 目标** — `fuzz/` 全部。
- **实验性 API 导出** — `backup`、`bulkload`、`vacuum` 从 facade crate root 移除。
- **Python/TypeScript 示例** — `examples/py-local/`、`examples/ts-local/`。
- **Makefile / lefthook** — 用纯 `scripts/`。

## Acceptance Criteria

0.1 is credible when:

- A Rust program can create and reopen a local graph database.
- Nodes, edges, labels, and properties persist across restart.
- Committed data survives kill/reopen recovery tests.
- One-hop and two-hop queries are documented and tested.
- Mini-Cypher results are deterministic for the supported surface.
- Ten realistic examples are documented and runnable.
- Rust API docs are clear enough to start without a server or non-Rust SDK.
- A manual large smoke can create 1,000,000 nodes and 5,000,000 edges without corruption on documented hardware.

## Product Bias

- Correctness before language breadth.
- Rust API before SDK expansion.
- WAL/recovery proof before feature count.
- Mini-Cypher before full Cypher.
- Fast focused validation before historical gate matrices.
