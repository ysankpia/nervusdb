# NervusDB（v2 / Full Roadmap Close-Out）

**一个嵌入式图数据库：像 SQLite 一样“打开路径就能用”，但为图遍历而生。**

> 当前处于 **全量 Roadmap 收尾执行阶段**：按 `M4 → M5 → Industrial` 分阶段推进，以 CI/TCK/质量门禁为发布依据。完成标准见 `docs/memos/DONE.md`。

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## 5 分钟上手（MVP）

### CLI

```bash
# 写入：CREATE / DELETE（输出 {"count":...}）
cargo run -p nervusdb-cli -- v2 write --db ./demo --cypher "CREATE (a {name: 'Alice'})-[:1]->(b {name: 'Bob'})"

# 查询：NDJSON（每行一条 JSON 记录）
cargo run -p nervusdb-cli -- v2 query --db ./demo --cypher "MATCH (a)-[:1]->(b) WHERE a.name = 'Alice' RETURN a, b LIMIT 10"
```

### Rust

```rust
use nervusdb::Db;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::open("/tmp/demo")?;
    db.execute("CREATE (n:Person {name: 'Alice'})", None)?;
    let rows = db.query("MATCH (n:Person) RETURN n", None)?;
    println!("rows={}", rows.len());
    Ok(())
}
```

### Python（PyO3，本地开发模式）

```bash
pip install maturin
maturin develop -m nervusdb-pyo3/Cargo.toml

python - <<'PY'
import nervusdb

db = nervusdb.open('/tmp/demo-py')
db.execute_write("CREATE (n:Person {name: 'Alice'})")
for row in db.query_stream("MATCH (n:Person) RETURN n LIMIT 1"):
    print(row)
db.close()
PY
```

> 注意：写语句（如 `CREATE/MERGE/DELETE/SET`）必须使用
> `execute_write(...)` 或写事务接口；用 `query(...)` 执行写语句会抛
> `ExecutionError`。

### Node（N-API 绑定）

```bash
cargo build --manifest-path nervusdb-node/Cargo.toml --release
npm --prefix examples/ts-local ci
npm --prefix examples/ts-local run smoke
```

v2 当前能力边界以 `docs/reference/cypher_support.md` 为准；是否“支持”以门禁结果为准。

## 执行路线图（收尾版）

- **M4**：TCK 分层门禁（Tier-0/1/2 PR 阻塞 + Tier-3 nightly）
- **M5**：Bindings（PyO3 + N-API）、文档对齐、对标基准、并发与 HNSW 调优
- **Industrial**：Fuzz / Chaos / Soak

详见 `docs/ROADMAP_2.0.md` 与 `docs/tasks.md`。

## Tier-3 全量通过率与 Beta 门禁

```bash
# 基于 Tier-3 全量日志生成通过率报告
TCK_FULL_LOG_FILE=tck_latest.log bash scripts/tck_full_rate.sh

# 按 Beta 阈值（默认 95%）阻断
TCK_MIN_PASS_RATE=95 bash scripts/beta_gate.sh
```

发布 Beta 前必须同时满足：官方全量 TCK ≥95% + 连续 7 天稳定窗 + 性能 SLO。

## v2 架构（当前事实）

- **两文件**：`<path>.ndb`（page store / segments / manifest）+ `<path>.wal`（redo log）
- **事务模型**：Single Writer + Snapshot Readers
- **存储形态**：MemTable（delta）+ 不可变 runs/segments（CSR）+ 显式 compaction/checkpoint
- **查询边界**：Query 通过 `nervusdb-api::{GraphStore, GraphSnapshot}` 访问图层

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
bash scripts/binding_smoke.sh
bash scripts/contract_smoke.sh
```

## Legacy（v1 已归档）

v1（含 redb 与旧绑定）位于 `_legacy_v1_archive/`，不参与 workspace/CI。

## 许可证

[Apache-2.0](LICENSE)
