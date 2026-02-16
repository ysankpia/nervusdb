# NervusDB Python Binding — 能力边界测试报告（最新）

> 更新日期: 2026-02-16  
> 测试入口: `examples-test/nervusdb-python-test/test_capabilities.py`

## 概要

| 指标 | 数值 |
|---|---:|
| 总测试数 | 138 |
| 通过 | 138 |
| 失败 | 0 |
| 跳过 | 0 |
| 结论 | Python 绑定在 examples-test 口径下与 Rust 基线能力对齐 |

## 覆盖范围

Python 测试覆盖 Node 镜像能力 + Python 专项能力，共 29 个分类：

1-20. 与 Node 对齐的核心能力（CRUD、子句、聚合、事务、错误处理、持久化等）  
21. `query_stream` 行为  
22. 参数化查询（`query/execute_write`）  
23. 向量能力（`set_vector/search_vector`）  
24. 类型化对象（Node/Relationship/Path）  
25. 异常层级  
26. `Db.path` + `open()` 入口  
27. Python 边界情况  
28. API 对齐（`open_paths` + maintenance）  
29. WriteTxn 低层 API 对齐

## 当前口径说明

- 本报告以 Rust 当前实现为唯一基线。
- Python 与 Node 不再用 skip 放行绑定差异。
- 绑定差异由 `binding_parity_gate.sh` 作为阻断门禁。

## 仍存在的问题（Rust 核心同态缺口，非 Python 绑定缺口）

以下问题在 Rust/Node/Python 三端一致存在：

1. 多标签子集匹配异常（例如 `MATCH (n:Manager)`）
2. `left()` / `right()` 未实现（`UnknownFunction`）
3. `shortestPath` 未完整支持
4. 部分 `MERGE` 关系场景稳定性问题

## 验证命令

```bash
bash examples-test/run_all.sh
bash scripts/binding_parity_gate.sh
```

## 关联文档

- `docs/binding-parity-matrix.md`
- `examples-test/nervusdb-rust-test/CAPABILITY-REPORT.md`
- `examples-test/nervusdb-node-test/CAPABILITY-REPORT.md`
