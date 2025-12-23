# NervusDB

NervusDB: An Embedded, Crash-Safe Graph Database (Subset of Cypher, Powered by Rust)

嵌入式三元组图数据库：**单文件 `redb` 存储 + 稳定 C ABI**，Rust 核心，绑定层只做参数搬运（Node/Python/WASM）。

## 快速开始（C / Rust）

### C（T10 stmt API：`prepare_v2 → step → column_* → finalize`）

```c
#include "nervusdb.h"
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>

static void die(nervusdb_error *err) {
  fprintf(stderr, "nervusdb error (%d): %s\n", err ? err->code : -1,
          err && err->message ? err->message : "<no message>");
  nervusdb_free_error(err);
  exit(1);
}

int main(void) {
  nervusdb_db *db = NULL;
  nervusdb_error *err = NULL;

  if (nervusdb_open("demo.redb", &db, &err) != NERVUSDB_OK)
    die(err);

  uint64_t alice, knows, bob;
  if (nervusdb_intern(db, "alice", &alice, &err) != NERVUSDB_OK)
    die(err);
  if (nervusdb_intern(db, "knows", &knows, &err) != NERVUSDB_OK)
    die(err);
  if (nervusdb_intern(db, "bob", &bob, &err) != NERVUSDB_OK)
    die(err);

  if (nervusdb_add_triple(db, alice, knows, bob, &err) != NERVUSDB_OK)
    die(err);

  nervusdb_stmt *stmt = NULL;
  if (nervusdb_prepare_v2(db, "MATCH (a)-[r]->(b) RETURN a, r, b", NULL, &stmt,
                          &err) != NERVUSDB_OK)
    die(err);

  for (;;) {
    nervusdb_status rc = nervusdb_step(stmt, &err);
    if (rc == NERVUSDB_ROW) {
      uint64_t a = nervusdb_column_node_id(stmt, 0);
      nervusdb_relationship r = nervusdb_column_relationship(stmt, 1);
      uint64_t b = nervusdb_column_node_id(stmt, 2);
      printf("a=%" PRIu64 "  r=(%" PRIu64 ",%" PRIu64 ",%" PRIu64 ")  b=%" PRIu64 "\n", a,
             r.subject_id, r.predicate_id, r.object_id, b);
      continue;
    }
    if (rc == NERVUSDB_DONE) {
      break;
    }
    die(err);
  }

  nervusdb_finalize(stmt);
  nervusdb_close(db);
  return 0;
}
```

> 重要：`nervusdb_column_*()` 返回的指针由 `stmt` 管理，**调用方禁止 free**；`column_text()` 的指针在下一次 `step()` 或 `finalize()` 后失效（见 `nervusdb-core/include/nervusdb.h` 注释）。

### Rust（core API）

```rust
use nervusdb_core::{Database, Fact, Options, QueryCriteria};

fn main() -> nervusdb_core::Result<()> {
    let mut db = Database::open(Options::new("demo.redb"))?;

    db.add_fact(Fact::new("alice", "knows", "bob"))?;

    let knows = db.resolve_id("knows")?.expect("missing predicate id");
    let triples: Vec<_> = db
        .query(QueryCriteria {
            subject_id: None,
            predicate_id: Some(knows),
            object_id: None,
        })
        .collect();

    println!("triples = {:?}", triples);
    Ok(())
}
```

## 仓库结构（高层）

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

## 单文件语义

- Rust/FFI 的 `open(path)` 会使用 `path.with_extension("redb")` 作为实际文件路径
- 所以传入 `demo.redb` 会生成/打开 `demo.redb`；传入 `demo.db` 会打开 `demo.redb`

## ABI 兼容性（1.0 起保证）

- 编译期：`NERVUSDB_ABI_VERSION`
- 运行期：`nervusdb_abi_version()` 必须等于上面的宏；不等就是你把头文件/动态库混用错了
- 仅当发生破坏性 ABI 变更才 bump `NERVUSDB_ABI_VERSION`（1.0 发布后至少 90 天内禁止改 `nervusdb.h` 签名）

## 安装 / 构建

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

## 核心特性

- **三索引三元组存储**：`SPO / POS / OSP`（写放大更小，但仍覆盖常见查询模式）
- **字典 Interning + LRU**：热字符串走内存缓存，避免反复 B-Tree 查找
- **事务与崩溃一致性**：`kill -9` 下通过 crash-test 门禁（PR smoke + nightly 1000x）
- **Cypher 查询支持（子集）**：提供 `exec_cypher(JSON)` + `stmt(step/column)` 两套 C API（支持范围见 `docs/cypher_support.md`）
- **Temporal（可选）**：Cargo feature `temporal`，默认关闭
- **绑定层薄包装**：Node.js (NAPI-RS)、Python (PyO3)、C (FFI)、WASM

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
# Benchmark 对比（NervusDB / SQLite / redb）
cargo run --example bench_compare -p nervusdb-core --release

# Cypher C API（JSON vs stmt）
cargo run --example bench_cypher_ffi -p nervusdb-core --release

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
- 任务/设计文档位于 `docs/task_progress.md` 与 `docs/design/`

## 许可证

[Apache-2.0](LICENSE)
