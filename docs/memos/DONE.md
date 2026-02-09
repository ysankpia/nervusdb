# NervusDB 完成标准（Definition of Done, v2.1）

本文件定义“全量 Roadmap 收尾”终点。满足即完成，不再无限扩展。

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

- [x] 主 CI 全绿（按当前本地同款门禁脚本回归通过）
- [x] crash-gate 可通过（`scripts/chaos_io_gate.sh` 已验证）
- [x] TCK Tier-0/Tier-1/Tier-2 为 PR 阻塞且可稳定通过
- [x] TCK Tier-3 nightly 有持续报告

### 2.3 质量门禁（Industrial）

- [x] fuzz 流程可运行且崩溃样例可回归（nightly workflow + regress 脚本；本地可 `FUZZ_ALLOW_SKIP=1`）
- [x] chaos IO 注入覆盖关键恢复路径
- [x] 24h soak 流程可运行并产出报告

### 2.4 性能与对标

- [x] NervusDB vs Neo4j/Memgraph 对标流程可执行
- [x] benchmark 报告（JSON + Markdown）版本化归档
- [x] 并发读 P95/P99 对比报告可追踪

## 3) 不做什么（本轮仍然禁止）

- 不引入新的 v3 破坏性文件格式
- 不在未过门禁前宣称“已支持”
- 不把一次性实验脚本当成发布能力
