# NervusDB v2.0 Roadmap（Execution Mode）

> **Vision**: 成为 AI/Edge 时代默认的 Embedded Graph Database。
>
> **Execution Principles**:
> 1. 以门禁定义“支持”（tests as contract）
> 2. 以阶段收敛推进（M4 → M5 → Industrial）
> 3. 主线始终可绿、可回滚

---

## Phase A：M4 收尾（Cypher/TCK）

**目标**：把 TCK 从 smoke 升级为分层门禁，持续提高 clauses/expressions 通过率。

### A.1 TCK Tiered Gates

- [x] Tier-0：core/extended smoke gate（已落地）
- [x] Tier-1：clauses 白名单门禁（PR 阻塞）
- [x] Tier-2：expressions 白名单门禁（PR 阻塞）
- [x] Tier-3：全量 TCK nightly（非阻塞 + 报告）

### A.2 失败聚类驱动修复

- [x] 自动产出失败聚类（按 feature / error pattern）
- [x] 每轮 PR 固定“拉入一批白名单 + 修一批失败簇”

### A.3 M4 完成标准

- [x] `M4-07`（clauses）从 WIP → Done
- [x] `M4-08`（expressions）从 WIP → Done
- [x] 在 `docs/tasks.md` 记录覆盖集与通过率

---

## Phase B：M5 交付（Bindings + Docs + Perf）

### B.1 M5-01 Bindings（PyO3 + N-API）

- [x] Python 异常分层：`NervusError/SyntaxError/ExecutionError/StorageError`
- [x] Python `Db.query_stream()` 迭代器接口
- [x] Node N-API scaffold：`open/query/beginWrite/commit/rollback`
- [x] 跨语言契约快测（Rust/Python/Node）

### B.2 M5-02 Docs Alignment

- [x] `README.md` / `README_CN.md` / `docs/reference/cypher_support.md` 对齐门禁事实
- [x] User Guide 补全 Rust/CLI/Python/Node 最小路径

### B.3 M5-03 Benchmark

- [x] NervusDB vs Neo4j vs Memgraph 对标入口（Docker）
- [x] JSON + Markdown 报告产物归档到 `docs/perf/`
- [x] 手动/定时 workflow（非阻塞主 CI）

### B.4 M5-04 Concurrency

- [x] 并发读热点 profile 与基线
- [x] 读路径优化（先低风险、再调度）
- [x] P95/P99 对比报告

### B.5 M5-05 HNSW Tuning

- [x] `M/efConstruction/efSearch` 可配置
- [x] recall-latency-memory 三维报告
- [x] 默认参数建议固化

---

## Phase C：Industrial Quality（Roadmap Phase 3）

### C.1 Fuzz

- [x] `cargo-fuzz` 目标接入（parser/planner/executor）
- [x] 崩溃样例归档与回归

### C.2 Chaos

- [x] IO 故障注入（磁盘满/权限失败）
- [x] WAL 恢复路径验证

### C.3 Soak

- [x] 24h 稳定性流程（nightly/scheduled）
- [x] 自动产物与失败复现信息

---

## 统一门禁矩阵

### PR 阻塞

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`
3. workspace 快速测试
4. TCK Tier-0/Tier-1/Tier-2
5. Python/Node smoke + 契约快测

### Nightly / Manual

1. TCK Tier-3 全量
2. benchmark 对标
3. chaos
4. soak
5. fuzz 长跑

---

## Done 定义（Roadmap 级）

- [x] `docs/tasks.md` 中 M4/M5/Industrial 全部 Done
- [x] `docs/memos/DONE.md` 全部勾选
- [x] 主 CI + crash-gate + industrial workflows 持续稳定
