# NervusDB Node Binding — 能力边界测试报告（最新）

> 更新日期: 2026-02-16  
> 测试入口: `examples-test/nervusdb-node-test/src/test-capabilities.ts`

## 概要

| 指标 | 数值 |
|---|---:|
| 总测试数 | 109 |
| 通过 | 109 |
| 失败 | 0 |
| 跳过 | 0 |
| 结论 | Node 绑定在 examples-test 口径下与 Rust 基线能力对齐 |

## 覆盖范围

当前 Node 能力测试覆盖 22 个分类：

1. 基础 CRUD
2. RETURN 投影
3. 多标签节点
4. WHERE 过滤
5. 查询子句
6. 聚合函数
7. MERGE
8. CASE 表达式
9. 字符串函数
10. 数学运算
11. 变长路径
12. EXISTS 子查询
13. FOREACH
14. 写事务（beginWrite/query/commit/rollback）
15. 错误处理（结构化 payload）
16. 关系方向
17. 复杂图模式
18. 批量写入性能
19. 持久化（close/reopen）
20. 边界情况
21. API 对齐（openPaths/maintenance/vector）
22. WriteTxn 低层 API 对齐

## 当前口径说明

- 本报告以 Rust 当前实现为唯一基线。
- Node 与 Python 不再使用 skip 掩盖绑定差异。
- `examples-test/run_all.sh` 要求 Rust/Node/Python 三端同时通过，否则整体失败。

## 仍存在的问题（Rust 核心同态缺口，非 Node 绑定缺口）

以下问题在 Rust/Node/Python 三端均可复现，属于核心引擎待修复项：

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
- `examples-test/nervusdb-python-test/CAPABILITY-REPORT.md`
