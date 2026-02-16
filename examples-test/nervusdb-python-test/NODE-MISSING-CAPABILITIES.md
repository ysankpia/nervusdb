# Node 绑定缺失能力报告（已归档）

> 状态: 已过时（保留此文件仅用于历史追溯）
> 最新口径请以以下文档为准：
>
> - `docs/binding-parity-matrix.md`
> - `examples-test/nervusdb-node-test/CAPABILITY-REPORT.md`
> - `examples-test/nervusdb-python-test/CAPABILITY-REPORT.md`

## 归档说明

本文件早期用于记录 Node 相对 Python 的缺失能力清单。
随着三端对齐推进，该文档中的“Node 缺失能力”结论已不再成立。

## 历史与当前对比（examples-test 口径）

| 维度 | 历史结论（过时） | 当前状态（2026-02-16） |
|---|---|---|
| Node 覆盖 | 99/105（有缺口） | 109/109（全通过） |
| Python 覆盖 | 131/135 | 138/138 |
| 三端一致性 | 存在绑定差异 | 已纳入 parity 阻断门禁 |

## 当前真实结论

- Node/Python 对 Rust 基线能力已完成对齐（examples-test 范围内）。
- 现阶段剩余问题主要是 Rust 核心同态缺口，不属于 Node 绑定单边缺失。
- 跨语言回归由 `scripts/binding_parity_gate.sh` 统一阻断。

## 使用建议

请不要再依据此归档文件判断当前能力状态；
所有最新结论请查看上方“最新口径”文档。
