# NervusDB

**Rust-first 嵌入式属性图数据库 — 图数据的 SQLite。**

打开一个本地路径，写入图数据，查询附近关系，崩溃后恢复并重新打开。
没有服务器，没有网络服务，没有平台仪式。

[![CI](https://github.com/ysankpia/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/ysankpia/nervusdb/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/nervusdb.svg)](https://crates.io/crates/nervusdb)
[![Downloads](https://img.shields.io/crates/d/nervusdb.svg)](https://crates.io/crates/nervusdb)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)

> [English](README.md)

## 当前重点

NervusDB 正在收缩到一个能完成的 0.1 主线：

- Rust 嵌入式 API
- 本地数据库目录存储
- Fjall 支撑的提交持久化和崩溃/重开 smoke
- 节点 / 边 / label / 属性持久化
- label scan 和邻居遍历
- 小而明确的 Mini-Cypher
- CLI 用于本地调试、文件驱动导入 smoke、查询和写入

完整 Cypher 兼容、多语言 SDK 稳定化、HNSW/向量搜索、跨绑定一致性门禁、
工业级 nightly gate 都属于历史或实验范围，不是 0.1 成功标准。

## 快速开始

### Rust

```rust
use nervusdb::Db;
use nervusdb::query::{prepare, query_collect, Params};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open("/tmp/nervusdb-demo")?;

    let snapshot = db.snapshot();
    let create = prepare("CREATE (n:Person {name: 'Alice'})")?;
    let mut txn = db.begin_write();
    create.execute_write(&snapshot, &mut txn, &Params::new())?;
    txn.commit()?;

    let rows = query_collect(
        &db.snapshot(),
        "MATCH (n:Person) RETURN n.name LIMIT 10",
        &Params::new(),
    )?;
    println!("{rows:?}");
    Ok(())
}
```

### CLI

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/nervusdb-demo \
  --cypher "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})"

cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/nervusdb-demo \
  --cypher "MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name LIMIT 10"
```

写语句必须使用 `prepare(...).execute_write(...)` 或 CLI write 路径。0.1 前的读查询应
保持在 Mini-Cypher 文档范围内。CLI query 输出是 newline-delimited JSON，CLI
write 输出类似 `{"count":3}` 的小 JSON 状态对象。0.1 的导入 smoke 使用现有
`v2 write --file` 输入；0.1 前没有稳定 import 命令。

## 架构

```text
nervusdb             public Rust crate
nervusdb::api        graph traits, shared IDs, storage-neutral boundaries
nervusdb::storage    Fjall-backed graph keyspaces, snapshots, recovery
nervusdb::query      Mini-Cypher parser/planner/executor for the 0.1 surface
nervusdb-cli         local debug/import/query/write tool
```

`nervusdb-api`、`nervusdb-storage`、`nervusdb-query` 可以作为本地 wrapper
crate 暂时留在 workspace 里，方便测试和脚本收口。它们不是 0.0.1 要发布的独立
公共包。

Python、Node.js、C 绑定、完整 openCypher TCK、向量搜索、一致性门禁、
perf/chaos/soak/fuzz 矩阵、release window 和 Fjall 之前的存储设计记录仍保留在
仓库中，但不属于默认产品路径。

## 开发

默认本地检查：

```bash
bash scripts/check.sh
```

它会运行格式化、公开 crate 和本地 wrapper clippy，以及 Mini-Cypher 核心快速测试。
更宽的验证手动运行：

```bash
cargo test --workspace
```

examples、crash recovery、benchmark、TCK、bindings、perf、fuzz、chaos、soak、
stability 相关检查只作为手动信号。

## 文档

- [文档索引](docs/index.md)
- [产品愿景](docs/product/vision.md)
- [0.1 范围](docs/product/scope-0.1.md)
- [架构总览](docs/architecture/overview.md)
- [测试策略](docs/engineering/testing-strategy.md)
- [Mini-Cypher 参考](docs/reference/mini-cypher.md)
- [本地验证](docs/runbooks/local-validation.md)

## 许可证

[AGPL-3.0](LICENSE)
