# R2：`executor.rs` 结构拆分（读写路径解耦）

更新时间：2026-02-12  
任务类型：Phase 1a  
任务状态：In Progress

## 1. 目标

- 拆分 `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs`。
- 按“写路径 -> 读路径 -> 排序投影”顺序完成模块化。
- 保持行为等价，不混入语义修复。

## 2. 边界

- 允许：内部函数迁移、模块拆分、重复逻辑收敛。
- 禁止：改动 query 语义、错误文案、对外接口签名。
- 禁止：在本任务处理 ReturnOrderBy2 语义缺陷。

## 3. 文件清单

### 3.1 必改文件

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor.rs`

### 3.2 新增文件（建议结构）

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/property_bridge.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/binding_utils.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/path_usage.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/label_constraint.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/write_support.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/merge_helpers.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/merge_execute_support.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/merge_overlay.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/write_path.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/read_path.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/projection_sort.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/join_apply.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/write_orchestration.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/create_delete_ops.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/merge_execution.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/foreach_ops.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/write_dispatch.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/txn_engine_impl.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/index_seek_plan.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/procedure_registry.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/plan_tail.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/plan_head.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/plan_mid.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/match_bound_rel_plan.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/match_in_undirected_plan.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/match_out_plan.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/plan_iterators.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/write_forwarders.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/plan_types.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/core_types.rs`（已完成）
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/executor/plan_dispatch.rs`（已完成）

## 4. 当前进展（2026-02-12）

1. 已完成切片-1（Property Bridge）
   - 从 `executor.rs` 提取属性桥接与类型转换函数簇到 `executor/property_bridge.rs`。
   - 新增模块级单测 2 条，覆盖递归 `Map/List` 转换与 map 批量转换。
2. 已完成切片-2（Binding Utils）
   - 从 `executor.rs` 提取 `optional_unbind / 节点绑定匹配 / 绑定包含判断` 到 `executor/binding_utils.rs`。
   - 新增模块级单测 4 条，覆盖 null 语义与绑定等价规则。
3. 已完成切片-3（Path Usage）
   - 从 `executor.rs` 提取路径去重辅助函数到 `executor/path_usage.rs`。
   - 该切片不引入新语义，仅模块迁移，回归由全量 quick gate 覆盖。
4. 已完成切片-4（Label Constraint）
   - 从 `executor.rs` 提取 `LabelConstraint` 及标签解析/匹配函数到 `executor/label_constraint.rs`。
   - 保持调用点不变，仅迁移类型与辅助逻辑。
5. 已完成切片-5（Write Support）
   - 从 `executor.rs` 提取写路径辅助函数 `merge_apply_set_items / merge_eval_props_on_row` 到 `executor/write_support.rs`。
   - 保持签名与调用点不变，仅做结构迁移。
6. 已完成切片-6（MERGE 写路径去重）
   - 删除 `execute_merge` 内部重复的 `apply_merge_set_items` 局部函数。
   - 统一复用 `executor/write_support.rs::merge_apply_set_items`，避免写路径重复实现漂移。
7. 已完成切片-7（MERGE Helper 模块化）
   - 从 `executor.rs` 提取 MERGE 辅助函数簇到 `executor/merge_helpers.rs`：
     `merge_find_node_candidates`、`merge_materialize_node_value`、`merge_collect_edges_between`、`merge_create_node` 等。
   - 保持原调用链不变，仅模块边界调整。
8. 已完成切片-8（MERGE 属性转换去重）
   - `execute_merge::find_existing_node` 删除本地 `to_api` 重复实现。
   - 统一复用 `merge_storage_property_to_api`，避免同一转换规则在多处漂移。
9. 已完成切片-9（MERGE 执行辅助模块化）
   - 将 `execute_merge` 内部本地辅助组（Overlay 缓存节点与 find/create 流程）抽离到 `executor/merge_execute_support.rs`。
   - 主流程改为调用 `exec_merge_find_or_create_node`，降低单函数内嵌复杂度。
10. 已完成切片-10（MERGE Overlay 类型抽离）
   - 将 `MergeOverlayNode/MergeOverlayEdge/MergeOverlayState` 从 `executor.rs` 抽离到 `executor/merge_overlay.rs`。
   - MERGE 读写辅助与主流程共享同一类型定义，减少文件内类型噪音。
11. 已完成切片-11（写路径函数簇模块化）
   - 将 `evaluate_property_value / execute_set / execute_set_from_maps / execute_remove / execute_set_labels / execute_remove_labels` 及写后 overlay 逻辑统一抽离到 `executor/write_path.rs`。
   - `executor.rs` 保留桥接函数与调用入口，继续对外保持同一行为与同一函数签名。
   - 当前 `executor.rs` 行数已降到 `5234`（重构前约 `6524`）。
12. 已完成切片-12（APPLY/Procedure 迭代器抽离）
   - 将 `ApplyIter / ProcedureCallIter` 从 `executor.rs` 抽离到 `executor/join_apply.rs`。
   - `PlanIterator` 对应分支与构造调用点保持不变，仅迁移类型定义与迭代实现。
   - 当前 `executor.rs` 行数已进一步降到 `5083`。
13. 已完成切片-13（聚合执行器抽离）
   - 将聚合执行逻辑 `execute_aggregate` 从 `executor.rs` 抽离到 `executor/projection_sort.rs`。
   - 聚合入口调用点不变，仍由 `Plan::Aggregate` 分支直接调用同名函数。
14. 已完成切片-14（读路径迭代器抽离）
   - 将 `MatchOutIter / MatchOutVarLenIter / ExpandIter` 从 `executor.rs` 抽离到 `executor/read_path.rs`。
   - `execute_plan` 中 `ExpandIter` 构造改为 `ExpandIter::new(...)`，避免暴露内部字段并保持调用行为一致。
   - 当前 `executor.rs` 行数已降到 `4228`。
15. 已完成切片-15（写编排主流程抽离）
   - 将 `execute_write_with_rows / execute_merge_with_rows / execute_merge_with_rows_inner` 从 `executor.rs` 抽离到 `executor/write_orchestration.rs`。
   - `executor.rs` 保留同名外壳函数，内部转发到新模块实现，确保外部调用路径不变。
   - 当前 `executor.rs` 行数已进一步降到 `3288`。
16. 已完成切片-16（CREATE/DELETE 执行簇抽离）
   - 将 `execute_create_from_rows / execute_delete_on_rows / execute_create_write_rows / execute_create / execute_delete` 及其删除目标收集辅助逻辑抽离到 `executor/create_delete_ops.rs`。
   - `executor.rs` 继续保留同名外壳函数转发，保持现有调用关系和外部行为不变。
   - 当前 `executor.rs` 行数已降到 `2834`。
17. 已完成切片-17（MERGE 执行主簇抽离）
   - 将 `execute_merge_create_from_rows / find_create_plan / execute_merge` 抽离到 `executor/merge_execution.rs`。
   - `executor.rs` 保留 `execute_merge_create_from_rows / execute_merge` 外壳转发，维持外部调用路径稳定。
   - 当前 `executor.rs` 行数已进一步降到 `2369`。
18. 已完成切片-18（FOREACH 执行簇抽离）
   - 将 `execute_foreach` 及其行注入辅助逻辑（`inject_rows`）抽离到 `executor/foreach_ops.rs`。
   - `executor.rs` 保留 `execute_foreach` 外壳转发，确保 `execute_write` 调用面不变。
   - 当前 `executor.rs` 行数已进一步降到 `2301`。
19. 已完成切片-19（写路径分发抽离）
   - 将 `execute_write` 递归分发逻辑抽离到 `executor/write_dispatch.rs`。
   - `executor.rs` 保留 `execute_write` 外壳转发，保持外部签名与调用入口不变。
   - 当前 `executor.rs` 行数已进一步降到 `2239`。
20. 已完成切片-20（WriteTxn 适配实现抽离）
   - 将 `WriteableGraph for EngineWriteTxn` 的实现从 `executor.rs` 内联模块迁移到 `executor/txn_engine_impl.rs`。
   - `WriteableGraph` trait 与公开导出保持原位，外部行为与适配语义不变。
   - 当前 `executor.rs` 行数已进一步降到 `2140`。
21. 已完成切片-21（IndexSeek 读路径抽离）
   - 将 `execute_plan` 中 `Plan::IndexSeek` 分支抽离到 `executor/index_seek_plan.rs`。
   - 主执行器保留原分支入口，仅改为调用模块函数，索引缺失回退逻辑保持不变。
   - 当前 `executor.rs` 行数已进一步降到 `2108`。
22. 已完成切片-22（Procedure 注册中心抽离）
   - 将 `Procedure / ErasedSnapshot / ProcedureRegistry / get_procedure_registry` 及内置过程实现抽离到 `executor/procedure_registry.rs`。
   - `executor.rs` 通过 `pub use` 暴露原有 API，保持过程调用入口与行为不变。
   - 当前 `executor.rs` 行数已进一步降到 `1961`。
23. 已完成切片-23（执行尾部分支抽离）
   - 将 `execute_plan` 尾部通用分支（`Skip / Limit / Distinct / Unwind / Union / 写语句防误用 / Values`）抽离到 `executor/plan_tail.rs`。
   - `executor.rs` 保留原分支入口，仅改为模块调用，保持分支行为与错误文案不变。
   - 当前 `executor.rs` 行数已进一步降到 `1863`。
24. 已完成切片-24（执行头部分支抽离）
   - 将 `execute_plan` 头部控制分支（`CartesianProduct / Apply / ProcedureCall / Foreach guard / NodeScan`）抽离到 `executor/plan_head.rs`。
   - `executor.rs` 保持原分支入口并调用新模块，行为与错误文案保持一致。
   - 当前 `executor.rs` 行数已进一步降到 `1795`。
25. 已完成切片-25（执行中段分支抽离）
   - 将 `execute_plan` 中段分支（`Filter / OptionalWhereFixup / Project / Aggregate / OrderBy`）抽离到 `executor/plan_mid.rs`。
   - `executor.rs` 保留原 match 入口，改为转发到新模块，输出语义保持一致。
   - 当前 `executor.rs` 行数已进一步降到 `1699`。
26. 已完成切片-26（绑定关系分支抽离）
   - 将 `execute_plan` 中 `Plan::MatchBoundRel` 分支抽离到 `executor/match_bound_rel_plan.rs`。
   - `executor.rs` 保留原分支入口并转发到新模块，关系方向与 optional/null 语义保持一致。
   - 当前 `executor.rs` 行数已进一步降到 `1610`。
27. 已完成切片-27（MatchIn/MatchUndirected 分支抽离）
   - 将 `execute_plan` 中 `Plan::MatchIn` 与 `Plan::MatchUndirected` 分支抽离到 `executor/match_in_undirected_plan.rs`。
   - `executor.rs` 保留分支入口，统一壳函数转发，行为与错误分类保持不变。
   - 当前 `executor.rs` 行数已进一步降到 `1327`。
28. 已完成切片-28（MatchOut/VarLen 分支抽离）
   - 将 `execute_plan` 中 `Plan::MatchOut` 与 `Plan::MatchOutVarLen` 分支抽离到 `executor/match_out_plan.rs`。
   - `executor.rs` 保留分支入口并转发到新模块，路径扩展、标签过滤与 optional 语义保持不变。
   - 当前 `executor.rs` 行数已进一步降到 `1262`。
29. 已完成切片-29（核心迭代器抽离）
   - 将 `NodeScanIter / FilterIter / CartesianProductIter` 从 `executor.rs` 抽离到 `executor/plan_iterators.rs`。
   - `PlanIterator` 与 `plan_head/plan_mid` 的调用面保持不变，仅迁移类型与迭代实现。
   - 当前 `executor.rs` 行数已进一步降到 `1155`。
30. 已完成切片-30（Plan 类型与迭代器定义抽离）
   - 将 `Plan / PlanIterator` 从 `executor.rs` 抽离到 `executor/plan_types.rs`。
   - `executor.rs` 通过 `pub use` 保持原有对外类型路径与调用面不变，仅迁移定义位置。
   - 当前 `executor.rs` 行数已进一步降到 `836`。
31. 已完成切片-31（Value/Row 核心类型抽离）
   - 将 `NodeValue / RelationshipValue / PathValue / ReifiedPathValue / Value / Row` 从 `executor.rs` 抽离到 `executor/core_types.rs`。
   - `executor.rs` 通过 `pub use` 保持原有类型路径不变，`Row.cols` 调整为 `pub(crate)` 以维持既有同 crate 访问面。
   - 当前 `executor.rs` 行数已进一步降到 `415`。
32. 已完成切片-32（Plan 分发主函数抽离）
   - 将 `execute_plan` 的主 `match` 分发体从 `executor.rs` 抽离到 `executor/plan_dispatch.rs`。
   - `executor.rs` 保留原入口函数签名并转发到新模块，外部调用路径与行为保持不变。
   - 当前 `executor.rs` 行数已进一步降到 `196`。
33. 回归结果
   - `cargo test -p nervusdb-query executor::property_bridge::tests --lib` 通过。
   - `cargo test -p nervusdb-query executor::binding_utils::tests --lib` 通过。
   - `cargo test -p nervusdb-query --lib` 通过。
   - `cargo test -p nervusdb --test t105_merge_test` 通过。
   - `cargo test -p nervusdb --test t323_merge_semantics` 通过。
   - `cargo test -p nervusdb --test t108_set_clause` 通过。
   - `cargo test -p nervusdb-query --lib`（切片-8后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-8后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-9后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-9后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-10后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-10后）通过。
   - `cargo fmt --all`（切片-11后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-11后）通过。
   - `cargo test -p nervusdb --test t108_set_clause`（切片-11后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-11后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-11后）通过（全绿）。
   - `cargo test -p nervusdb-query --lib`（切片-12后）通过。
   - `cargo test -p nervusdb --test t319_subquery`（切片-12后）通过。
   - `cargo test -p nervusdb --test t320_procedures`（切片-12后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-13后）通过。
   - `cargo test -p nervusdb --test t152_aggregation`（切片-13后）通过。
   - `cargo test -p nervusdb --test t62_order_by_skip_test`（切片-13后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-14后）通过。
   - `cargo test -p nervusdb --test t318_paths`（切片-14后）通过。
   - `cargo test -p nervusdb --test t333_varlen_direction`（切片-14后）通过。
   - `cargo test -p nervusdb --test t151_optional_match`（切片-14后）通过。
   - `cargo test -p nervusdb --test t321_incoming`（切片-14后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-14后）通过（全绿）。
   - `cargo test -p nervusdb-query --lib`（切片-15后）通过。
   - `cargo test -p nervusdb --test t108_set_clause`（切片-15后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-15后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-15后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-15后）通过。
   - `bash scripts/binding_smoke.sh`（切片-15后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-16后）通过。
   - `cargo test -p nervusdb --test create_test`（切片-16后）通过。
   - `cargo test -p nervusdb --test t108_set_clause`（切片-16后）通过。
   - `bash scripts/contract_smoke.sh` 通过。
   - `bash scripts/binding_smoke.sh` 通过（保留既有 `pyo3 gil-refs` warning）。
   - `bash scripts/workspace_quick_test.sh` 通过（全绿）。
   - `cargo test -p nervusdb-query --lib`（切片-17后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-17后）通过。
   - `cargo test -p nervusdb --test create_test`（切片-17后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-17后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-17后）通过。
   - `bash scripts/binding_smoke.sh`（切片-17后）通过（保留既有 `pyo3 gil-refs` warning）。
   - `cargo test -p nervusdb-query --lib`（切片-18后）通过。
   - `cargo test -p nervusdb --test t324_foreach`（切片-18后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-19后）通过。
   - `cargo test -p nervusdb --test t108_set_clause`（切片-19后）通过。
   - `cargo test -p nervusdb --test t324_foreach`（切片-19后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-20后）通过。
   - `cargo test -p nervusdb --test create_test`（切片-20后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-21后）通过。
   - `cargo test -p nervusdb --test t156_optimizer`（切片-21后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-21后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-21后）通过。
   - `bash scripts/binding_smoke.sh`（切片-21后）通过（保留既有 `pyo3 gil-refs` warning）。
   - `cargo test -p nervusdb-query --lib`（切片-22后）通过。
   - `cargo test -p nervusdb --test t320_procedures`（切片-22后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-23后）通过。
   - `cargo test -p nervusdb --test t306_unwind`（切片-23后）通过。
   - `cargo test -p nervusdb --test t307_union`（切片-23后）通过。
   - `cargo test -p nervusdb --test t62_order_by_skip_test`（切片-23后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-24后）通过。
   - `cargo test -p nervusdb --test t64_node_scan_test`（切片-24后）通过。
   - `cargo test -p nervusdb --test t319_subquery`（切片-24后）通过。
   - `cargo test -p nervusdb --test t320_procedures`（切片-24后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-25后）通过。
   - `cargo test -p nervusdb --test t305_with_clause`（切片-25后）通过。
   - `cargo test -p nervusdb --test t152_aggregation`（切片-25后）通过。
   - `cargo test -p nervusdb --test t62_order_by_skip_test`（切片-25后）通过。
   - `cargo test -p nervusdb --test t304_remove_clause`（切片-25后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-26后）通过。
   - `cargo test -p nervusdb --test t321_incoming`（切片-26后）通过。
   - `cargo test -p nervusdb --test t334_named_path`（切片-26后）通过。
   - `cargo test -p nervusdb --test t325_pattern_props`（切片-26后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-26后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-26后）通过。
   - `bash scripts/binding_smoke.sh`（切片-26后）通过（保留既有 `pyo3 gil-refs` warning）。
   - `cargo test -p nervusdb-query --lib`（切片-27后）通过。
   - `cargo test -p nervusdb --test t321_incoming`（切片-27后）通过。
   - `cargo test -p nervusdb --test t315_direction`（切片-27后）通过。
   - `cargo test -p nervusdb --test t334_named_path`（切片-27后）通过。
   - `cargo test -p nervusdb --test t325_pattern_props`（切片-27后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-27后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-27后）通过。
   - `bash scripts/binding_smoke.sh`（切片-27后）通过（保留既有 `pyo3 gil-refs` warning）。
   - `cargo test -p nervusdb-query --lib`（切片-28后）通过。
   - `cargo test -p nervusdb --test t52_query_api`（切片-28后）通过。
   - `cargo test -p nervusdb --test t333_varlen_direction`（切片-28后）通过。
   - `cargo test -p nervusdb --test t60_variable_length_test`（切片-28后）通过。
   - `cargo test -p nervusdb --test t321_incoming`（切片-28后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-29后）通过。
   - `cargo test -p nervusdb --test t52_query_api`（切片-29后）通过。
   - `cargo test -p nervusdb --test t305_with_clause`（切片-29后）通过。
   - `cargo test -p nervusdb --test t317_joins`（切片-29后）通过。
   - `cargo test -p nervusdb --test t64_node_scan_test`（切片-29后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-29后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-29后）通过。
   - `bash scripts/binding_smoke.sh`（切片-29后）通过（保留既有 `pyo3 gil-refs` warning）。
   - `cargo clippy --workspace --exclude nervusdb-pyo3 --all-targets -- -W warnings`（切片-29后）通过（保留既有 `evaluator.rs/write_orchestration.rs` clippy warning 簇）。
   - `bash scripts/tck_tier_gate.sh tier0`（切片-29后）通过。
   - `bash scripts/tck_tier_gate.sh tier1`（切片-29后）通过。
   - `bash scripts/tck_tier_gate.sh tier2`（切片-29后）通过。
   - `cargo fmt --all`（切片-30后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-30后）通过。
   - `cargo test -p nervusdb --test create_test`（切片-30后）通过。
   - `cargo test -p nervusdb --test t324_foreach`（切片-30后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-30后）通过。
   - `cargo fmt --all`（切片-31后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-31后）通过。
   - `cargo test -p nervusdb --test create_test`（切片-31后）通过。
   - `cargo test -p nervusdb --test t324_foreach`（切片-31后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-31后）通过。
   - `cargo fmt --all`（切片-32后）通过。
   - `cargo test -p nervusdb-query --lib`（切片-32后）通过。
   - `cargo test -p nervusdb --test create_test`（切片-32后）通过。
   - `cargo test -p nervusdb --test t324_foreach`（切片-32后）通过。
   - `cargo test -p nervusdb --test t323_merge_semantics`（切片-32后）通过。
   - `bash scripts/workspace_quick_test.sh`（切片-32后）通过（全绿）。
   - `bash scripts/contract_smoke.sh`（切片-32后）通过。
   - `bash scripts/binding_smoke.sh`（切片-32后）通过（保留既有 `pyo3 gil-refs` warning）。

## 5. TDD 拆分步骤

1. 写路径失败测试：SET/DELETE/MERGE 路径等价性。
2. 写路径拆分并通过回归。
3. 读路径失败测试：MATCH/OPTIONAL MATCH/VARLEN。
4. 读路径拆分并通过回归。
5. 排序投影失败测试：ORDER BY/SKIP/LIMIT/RETURN。
6. 排序投影拆分并通过回归。

## 6. 测试清单

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t105_merge_test.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t108_set_clause.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t323_merge_semantics.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t62_order_by_skip_test.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t151_optional_match.rs`

## 7. 风险与回滚

- 风险：执行器内部状态顺序变化影响副作用。
- 检测：同 query 同数据集对照节点/边变更计数与结果集。
- 回滚：出现 P0 即整 PR 回滚。

## 8. 完成定义（DoD）

- executor 拆分后模块职责单一。
- 全门禁通过且行为等价。
- 未引入新的 public type/错误类别。
