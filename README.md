# NervusDB

嵌入式三元组知识图谱数据库，纯 Rust 实现，专注于本地/边缘环境下的知识管理、链式联想与轻量推理。支持六序索引（Hexastore）、时序存储、Cypher 查询以及图算法。

## 项目结构

```
nervusdb/
├── nervusdb-core/       # Rust 核心库
│   ├── src/
│   │   ├── lib.rs       # 主入口：Database、Options、QueryCriteria
│   │   ├── storage/     # 存储层：Hexastore、TemporalStore
│   │   ├── query/       # Cypher 查询解析器和执行器
│   │   ├── algorithms/  # 图算法：路径查找、中心性分析
│   │   ├── ffi.rs       # C FFI 接口
│   │   └── migration/   # 旧版数据迁移工具
│   └── include/nervusdb.h  # C 头文件
├── bindings/
│   ├── node/            # Node.js 绑定 (NAPI-RS)
│   └── python/          # Python 绑定 (PyO3)
├── nervusdb-wasm/       # WebAssembly 模块
└── examples/
    └── c/               # C 语言示例
```

## 安装

### Rust (Cargo)

```toml
[dependencies]
nervusdb-core = { git = "https://github.com/LuQing-Studio/nervusdb" }
```

### 从源码构建

```bash
git clone https://github.com/LuQing-Studio/nervusdb.git
cd nervusdb
cargo build --release
```

## 快速上手

### Rust

```rust
use nervusdb_core::{Database, Options, Fact};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 打开数据库
    let mut db = Database::open(Options::new("demo.nervusdb"))?;

    // 添加事实
    let alice = db.intern("alice")?;
    let knows = db.intern("knows")?;
    let bob = db.intern("bob")?;

    db.add_fact(Fact {
        subject: "alice",
        predicate: "knows",
        object: "bob",
        properties: None,
    })?;

    // 执行 Cypher 查询
    let results = db.execute_query("MATCH (a)-[:knows]->(b) RETURN a, b")?;
    println!("{:?}", results);

    Ok(())
}
```

### C

```c
#include "nervusdb.h"

int main() {
    nervusdb_db *db;
    nervusdb_error *err = NULL;

    nervusdb_open("demo.nervusdb", &db, &err);

    uint64_t alice, knows, bob;
    nervusdb_intern(db, "alice", &alice, &err);
    nervusdb_intern(db, "knows", &knows, &err);
    nervusdb_intern(db, "bob", &bob, &err);

    nervusdb_add_triple(db, alice, knows, bob, &err);
    nervusdb_close(db);

    return 0;
}
```

## 核心特性

- **六序索引 (Hexastore)**：SPO/SOP/PSO/POS/OSP/OPS 六种索引顺序，高效支持任意模式查询
- **时序存储 (Temporal Store)**：支持 Episode、Fact 时间线追溯与查询
- **Cypher 查询**：支持 MATCH/WHERE/RETURN/WITH/ORDER BY/LIMIT 等语法
- **图算法**：内置 BFS/DFS/Dijkstra/A* 路径查找、PageRank/度中心性分析
- **事务支持**：`begin_transaction()`/`commit_transaction()`/`abort_transaction()`
- **多语言绑定**：Node.js (NAPI-RS)、Python (PyO3)、C (FFI)、WebAssembly

## 开发与测试

```bash
# 格式检查
cargo fmt --all -- --check

# Lint 检查
cargo clippy --workspace --all-targets

# 运行测试
cargo test --workspace

# 构建 release
cargo build --workspace --release
```

### 运行示例

```bash
# Hexastore 基准测试
cargo run --example bench_hexastore -p nervusdb-core

# 时序存储基准测试
cargo run --example bench_temporal -p nervusdb-core
```

## 数据迁移

从旧版目录格式迁移到新版 redb 单文件格式：

```bash
cargo run --bin nervus-migrate --features migration-cli -- <旧目录> <新文件>
```

## 贡献

- Issue/PR 遵循 GitHub 流程
- pre-commit 和 pre-push 钩子已启用 (cargo fmt / clippy / test)
- 架构文档位于 `docs/architecture/`

## 许可证

[Apache-2.0](LICENSE)
