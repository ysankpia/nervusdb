# NervusDB

**Rust 原生嵌入式属性图数据库 — 图数据的 SQLite。**

打开一个路径，即可获得完整的图数据库。支持 Cypher 查询。
无需服务器、无需配置、无外部依赖。

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)

> [English](README.md)

## 核心特性

- **嵌入式** — 打开路径即用，无守护进程，无网络通信。
- **Cypher** — openCypher TCK 100% 通过率（3 897 / 3 897 场景）。
- **崩溃安全** — 基于 WAL 的存储，单写者 + 快照读者事务模型。
- **多平台绑定** — Rust、Python (PyO3)、Node.js (N-API)、CLI。
- **向量搜索** — 内置 HNSW 索引，支持图 + 向量混合查询。

## 快速开始

### Rust

```rust
use nervusdb::Db;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open("/tmp/demo")?;
    db.execute("CREATE (n:Person {name: 'Alice'})", None)?;
    let rows = db.query("MATCH (n:Person) RETURN n.name", None)?;
    println!("{} row(s)", rows.len());
    Ok(())
}
```

### Python

```bash
pip install maturin
maturin develop -m nervusdb-pyo3/Cargo.toml
```

```python
import nervusdb

db = nervusdb.open("/tmp/demo-py")
db.execute_write("CREATE (n:Person {name: 'Alice'})")
for row in db.query_stream("MATCH (n:Person) RETURN n.name"):
    print(row)
db.close()
```

### Node.js

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
```

```typescript
const { Db } = require("./nervusdb-node");

const db = Db.open("/tmp/demo-node");
db.executeWrite("CREATE (n:Person {name: 'Alice'})");
const rows = db.query("MATCH (n:Person) RETURN n.name");
console.log(rows);
db.close();
```

### CLI

```bash
cargo run -p nervusdb-cli -- v2 write \
  --db /tmp/demo \
  --cypher "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})"

cargo run -p nervusdb-cli -- v2 query \
  --db /tmp/demo \
  --cypher "MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name"
```

> 写语句（`CREATE`、`MERGE`、`DELETE`、`SET`）必须使用 `execute_write` /
> `executeWrite` 或写事务接口。用 `query()` 执行写语句会抛出错误。

## 测试状态

| 测试套件 | 用例数 | 状态 |
|----------|--------|------|
| openCypher TCK | 3 897 / 3 897 | 100% |
| Rust 单元 + 集成测试 | 153 | 全部通过 |
| Python (PyO3) | 138 | 全部通过 |
| Node.js (N-API) | 109 | 全部通过 |

## 文档

- [用户指南](docs/user-guide.md) — 全平台 API 参考
- [架构设计](docs/architecture.md) — 存储、查询管线、crate 结构
- [Cypher 支持](docs/cypher-support.md) — 完整合规矩阵
- [路线图](docs/ROADMAP.md) — 当前与计划阶段
- [CLI 参考](docs/cli.md) — 命令行用法
- [绑定对等](docs/binding-parity.md) — 跨平台 API 覆盖

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
bash scripts/binding_smoke.sh
```

## 许可证

[AGPL-3.0](LICENSE)
