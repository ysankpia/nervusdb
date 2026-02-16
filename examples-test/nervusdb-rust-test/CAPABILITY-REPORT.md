# NervusDB Rust 核心引擎能力测试报告（最新）

> 更新日期: 2026-02-16  
> 测试入口: `examples-test/nervusdb-rust-test/tests/test_capabilities.rs`

## 概要

| 指标 | 数值 |
|---|---:|
| 总测试数 | 153 |
| 通过 | 153 |
| 失败 | 0 |
| 跳过 | 0 |
| 结论 | Rust 基线能力测试全绿 |

## 覆盖范围

Rust 能力测试覆盖 35 个分类：

- 1-20: 与 Node/Python 的共享能力面（CRUD、子句、聚合、事务、路径、错误处理等）
- 21-35: Rust 额外能力面（ReadTxn/DbSnapshot/Params/execute_mixed/ExecuteOptions/backup/vacuum/bulkload/index/vector/reify/open_paths 等）

## 口径说明

- 本报告是 Node/Python 绑定对齐的基准来源。
- 某些已知核心缺口在测试中以“确认同态行为”方式保留，不视为绑定失败。

## 已知核心缺口（待内核修复）

1. 多标签子集匹配异常（例如 `MATCH (n:Manager)`）
2. `left()` / `right()` 未实现（`UnknownFunction`）
3. `shortestPath` 未完整支持
4. 部分 `MERGE` 关系场景稳定性问题

## 验证命令

```bash
bash examples-test/run_all.sh
cargo test -p nervusdb-rust-test --test test_capabilities -- --test-threads=1 --nocapture
```

## 关联文档

- `docs/binding-parity-matrix.md`
- `examples-test/nervusdb-node-test/CAPABILITY-REPORT.md`
- `examples-test/nervusdb-python-test/CAPABILITY-REPORT.md`
