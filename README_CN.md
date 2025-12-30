# NervusDB（v2 / Scope Frozen）

**一个嵌入式图数据库：像 SQLite 一样“打开路径就能用”，但为图遍历而生。**

> 本仓库进入收尾模式：**冻结范围，不再无限加功能**。完成标准见 `docs/memos/DONE.md`，规格见 `docs/spec.md`。

[![CI](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml/badge.svg)](https://github.com/LuQing-Studio/nervusdb/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## 5 分钟上手（MVP）

当前 MVP 收敛为 **v2（`.ndb + .wal`，Rust-first）**：目标是让陌生开发者用 CLI 跑通写入/查询，并且通过 crash gate。

```bash
# 写入：CREATE / DELETE（输出 {"count":...}）
cargo run -p nervusdb-cli -- v2 write --db ./demo --cypher "CREATE (a {name: 'Alice'})-[:1]->(b {name: 'Bob'})"

# 查询：NDJSON（每行一条 JSON 记录）
cargo run -p nervusdb-cli -- v2 query --db ./demo --cypher "MATCH (a)-[:1]->(b) WHERE a.name = 'Alice' RETURN a, b LIMIT 10"
```

v2 的边界（白名单之外必须 fail-fast）以 `docs/reference/cypher_support.md` 为准。

## 这项目“什么时候算完”？

别自欺欺人：如果没有终点线，你会一直写下去，直到你厌恶自己。

终点线已经写死在：`docs/memos/DONE.md`。

## v2 架构（真实的，不吹牛）

- **两文件**：`<path>.ndb`（page store / segments / manifest）+ `<path>.wal`（redo log）
- **事务模型**：Single Writer + Snapshot Readers
- **存储形态**：MemTable（delta）+ 不可变 runs/segments（CSR）+ 显式 compaction/checkpoint
- **查询边界**：Query 只能通过 `nervusdb-v2-api::{GraphStore, GraphSnapshot}` 读图，不准摸 pager/WAL

仓库结构见 `docs/reference/project-structure.md`。

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -W warnings
cargo test --workspace
./scripts/v2_bench.sh
```

## Legacy（v1 已归档）

v1（含 redb 与旧绑定）已移到 `_legacy_v1_archive/`，不参与 workspace/CI，也不再维护。

## 许可证

[Apache-2.0](LICENSE)
