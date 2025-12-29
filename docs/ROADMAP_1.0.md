# NervusDB 1.0 落地路线图 (Rust-First Edition)

> **目标**：打造 Rust 生态中最优秀的嵌入式图数据库 ("The SQLite of Graph DBs for Rust")。
> **策略**：暂缓多语言绑定，集中火力打磨 Rust Native 体验 (API, Docs, CLI)。

## 1. 核心理念：什么是 Rust 界的 "SQLite 体验"？

1.  **极简集成**: `cargo add nervusdb` → `use nervusdb::prelude::*;` → `Db::open("graph.ndb")`。
2.  **零配置**: 默认合理的参数，不需要调优即可用于生产。
3.  **可调试性**: 必须有一个强大的 CLI 工具 (`nervusdb-cli`) 用于查看数据、执行 Ad-hoc 查询。
4.  **类型安全**: 充分利用 Rust 类型系统 (Serde 支持, 强类型参数)。

## 2. 1.0 落地行动计划 (Action Roadmap)

### 🚀 Phase 1: API 与 开发者体验 (The Rust DX)
**目标**：让 Rust 开发者用得爽。

- [ ] **R1. Facade API 清洗**:
    - 审查 `nervusdb-v2` 的 `pub` 导出。确保没有内部类型泄露。
    - 确保 `Db`, `Txn`, `Query` 的命名和用法符合 Rust 惯例（类似 `rusqlite` 或 `sled`）。
    - 增加 Feature Flags (`async`, `serde`, `full`) 管理。
- [ ] **R2. 示例工程 (Examples)**:
    - `examples/hello_world.rs`: 基础增删改查。
    - `examples/social_network.rs`: 复杂图查询演示。
    - `examples/axum_integration.rs`: Web 服务集成演示。
- [ ] **R3. CLI 增强**:
    - 让 `nervusdb-cli` 支持 REPL (Read-Eval-Print Loop)。
    - 支持 `.schema` 查看元数据。

### � Phase 2: 文档与生态 (Docs & Ecosystem)
**目标**：消除上手门槛。

- [ ] **R4. RustDoc 覆盖**:
    - 所有 `pub` item 必须有文档。
    - 顶层 crate 文档必须包含 Quickstart。
- [ ] **R5. The NervusDB Book**:
    - 类似 `mdBook` 的简明教程（原理、最佳实践、Cypher 语法速查）。
- [ ] **R6. Crates.io 发布准备**:
    - 清理 `Cargo.toml` 元数据 (License, Keywords, Repository)。
    - 确保 `cargo publish --dry-run` 通过。

### �️ Phase 3: 质量与发布 (Quality & Release)
**目标**：建立信任。

- [ ] **R7. 模糊测试 (Fuzzing)**:
    - 集成 `arbitrary` 和 `libfuzzer`，对 `nervusdb-v2-query` 进行语法树变异攻击。
- [ ] **R8. 性能基准 (Benchmarks)**:
    - 在 README 中展示真实场景下的 RPS (Reads/Writes Per Second)。

## 3. 立即执行 (Next Steps)

1.  **重置任务板**: 生成新的 `docs/tasks.md`，聚焦于 Rust API 和 CLI。
2.  **Demo 驱动开发**: 写一个 `examples/tour.rs`，模拟用户第一次使用的全过程，发现 API 的痛点。
