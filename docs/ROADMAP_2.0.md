# NervusDB v2.0 Roadmap（SQLite-Beta 收敛模式）

> **Vision**: 在单机嵌入式场景达到“图数据库界 SQLite（Beta）”。
>
> **Execution Principles**:
> 1. 以门禁定义“支持”（tests as contract）
> 2. 以阶段裁决推进（功能 → 稳定 → 性能）
> 3. 主线始终可绿、可回滚

---

## Phase A：功能线（TCK 全量冲 95%）

**目标**：在分层门禁基础上，把 Tier-3 官方全量通过率提升到 **≥95%**。

### A.1 TCK Tiered Gates

- [x] Tier-0：core/extended smoke gate（已落地）
- [x] Tier-1：clauses 白名单门禁（PR 阻塞）
- [x] Tier-2：expressions 白名单门禁（PR 阻塞）
- [x] Tier-3：全量 TCK nightly（非阻塞 + 报告）
- [x] Tier-3 通过率报告（`scripts/tck_full_rate.sh`）
- [x] 95% 阈值 gate（`scripts/beta_gate.sh`，manual/nightly 阻断）

### A.2 失败聚类驱动修复

- [x] 自动产出失败聚类（按 feature / error pattern）
- [x] 每轮 PR 固定“拉入一批白名单 + 修一批失败簇”

### A.3 M4 完成标准

- [x] `M4-07`（clauses）从 WIP → Done
- [x] `M4-08`（expressions）从 WIP → Done
- [x] 在 `docs/tasks.md` 记录覆盖集与通过率
- [ ] Tier-3 官方全量通过率 ≥95%

---

## Phase B：稳定线（冻结 + 7天稳定窗）

### B.1 接口与兼容冻结

- [x] Python 异常分层：`NervusError/SyntaxError/ExecutionError/StorageError/CompatibilityError`
- [x] Node 结构化错误 payload（`code/category/message`）
- [x] `storage_format_epoch` 校验与 `StorageFormatMismatch` 上抛
- [ ] 冻结后禁止破坏公共 API（Rust/CLI/Python/Node）

### B.2 稳定门禁

- [ ] 连续 7 天：主 CI 全绿
- [ ] 连续 7 天：nightly（TCK/benchmark/chaos/soak/fuzz）无阻断失败
- [ ] 任一阻断失败自动重置稳定窗计数

---

## Phase C：性能线（大规模 SLO 封板）

- [ ] 读查询 P99 <= 120ms
- [ ] 写事务 P99 <= 180ms
- [ ] 向量检索 P99 <= 220ms
- [ ] 任一不达标则不发布 Beta

---

## Industrial（持续质量护栏）

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
2. TCK 通过率统计 + 95% gate
3. benchmark 对标
4. chaos
5. soak
6. fuzz 长跑

---

## Done 定义（SQLite-Beta 级）

- [ ] 官方全量 TCK 通过率 ≥95%
- [ ] 连续 7 天主 CI + nightly 稳定
- [ ] 大规模性能 SLO 全达标
