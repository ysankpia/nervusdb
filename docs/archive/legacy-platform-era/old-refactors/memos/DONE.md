# NervusDB 完成标准（Definition of Done, v2.2 / SQLite-Beta）

本文件定义“图数据库界 SQLite（Beta）”终点。满足即可发布 Beta。

## 1) 目标范围

当前完成定义覆盖：

- M4：Cypher/TCK 门禁收敛
- M5：Bindings + Docs + Benchmark + Concurrency + HNSW 调优
- Industrial：Fuzz + Chaos + Soak

事实来源：

- `docs/spec.md`
- `docs/tasks.md`
- `docs/ROADMAP_2.0.md`

## 2) 完成标准（全部满足）

### 2.1 用户路径（5~10 分钟可复现）

- [x] `README.md` 顶部示例可复制粘贴运行
- [x] `README_CN.md` 与英文 README 口径一致
- [x] `docs/reference/cypher_support.md` 与实际门禁结果一致
- [x] Rust/CLI/Python/Node 至少各有一条最小可运行路径

### 2.2 工程门禁

- [ ] 主 CI 连续 7 天全绿
- [x] crash-gate 可通过（`scripts/chaos_io_gate.sh` 已验证）
- [x] TCK Tier-0/Tier-1/Tier-2 为 PR 阻塞且可稳定通过
- [x] TCK Tier-3 nightly 有持续报告
- [x] Tier-3 通过率统计与 95% gate 已接入（`scripts/tck_full_rate.sh` + `scripts/beta_gate.sh`）
- [ ] 官方全量 TCK 通过率 ≥95%（当前基线：2026-02-10 Tier-3 全量统计为 50.28%，1945/3868，见 `artifacts/tck/tier3-rate.json`）

### 2.3 质量门禁（Industrial）

- [x] fuzz 流程可运行且崩溃样例可回归（nightly workflow + regress 脚本；本地可 `FUZZ_ALLOW_SKIP=1`）
- [x] chaos IO 注入覆盖关键恢复路径
- [x] 24h soak 流程可运行并产出报告
- [ ] 连续 7 天 nightly（TCK/benchmark/chaos/soak/fuzz）无阻断失败

### 2.4 性能与对标

- [x] NervusDB vs Neo4j/Memgraph 对标流程可执行
- [x] benchmark 报告（JSON + Markdown）版本化归档
- [x] 并发读 P95/P99 对比报告可追踪
- [ ] 大规模发布 SLO 达标（读120ms/写180ms/向量220ms，P99）

### 2.5 存储与兼容

- [x] `storage_format_epoch` 已落地且默认强校验
- [x] epoch 不匹配时返回 `StorageFormatMismatch`
- [x] Rust/Python/Node 错误语义包含 Compatibility 分类

## 3) 不做什么（本轮仍然禁止）

- 不引入新的 v3 破坏性文件格式
- 不在未过门禁前宣称“已支持”
- 不把一次性实验脚本当成发布能力
