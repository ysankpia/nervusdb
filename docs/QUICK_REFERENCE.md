# 快速参考（当前）

## 常用门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings
bash scripts/workspace_quick_test.sh
bash scripts/tck_tier_gate.sh tier0
bash scripts/tck_tier_gate.sh tier1
bash scripts/tck_tier_gate.sh tier2
bash scripts/binding_smoke.sh
bash scripts/contract_smoke.sh
```

## 文档入口

- 规范：`docs/spec.md`
- 任务：`docs/tasks.md`
- 路线图：`docs/ROADMAP_2.0.md`
- 完成定义：`docs/memos/DONE.md`
- 用户指南：`docs/user-guide.md`

历史版本已归档：`docs/archive/legacy-2026Q1/QUICK_REFERENCE.md`。
