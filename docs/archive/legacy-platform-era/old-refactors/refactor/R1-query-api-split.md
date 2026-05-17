# R1：`query_api.rs` 结构拆分（行为等价）

更新时间：2026-02-12  
任务类型：Phase 1a  
任务状态：In Progress

## 1. 目标

- 将 `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs` 从单体文件拆分为可维护模块。
- 保持对外入口函数、返回类型、错误分类完全不变。
- 通过全门禁确认行为等价。

## 2. 边界

- 允许：内部模块重组、私有函数迁移、模块 re-export。
- 禁止：对外函数签名变化、错误类别变化、语义修复混入。
- 禁止：顺手修改 `executor.rs`/`evaluator.rs` 业务逻辑。

## 3. 文件清单

### 3.1 必改文件

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/lib.rs`

### 3.2 新增文件（建议结构）

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/mod.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/entry.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/parse.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/validate.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/assemble.rs`

## 4. TDD 拆分步骤

1. 新增失败测试：覆盖 parse/validate/assemble 入口等价行为。
2. 最小实现：先搬迁私有 helper 到子模块，保持旧入口委托。
3. 重构：缩减 `query_api.rs`，只保留 re-export 与薄入口。
4. 回归：跑全门禁 + 受影响测试清单。

## 5. 测试清单

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t52_query_api.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t311_expressions.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t62_order_by_skip_test.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb/tests/t332_binding_validation.rs`

## 6. 风险与回滚

- 风险：入口组装顺序变化触发隐式语义变化。
- 检测：对照 R0 样本查询结果与错误类别。
- 回滚：单 PR 回滚，不跨任务修复。

## 7. 完成定义（DoD）

- `query_api.rs` 明显减重，职责拆分清晰。
- 全门禁通过，且受影响测试无新增失败。
- 对外入口与错误模型保持不变。

## 8. 当前进展（2026-02-12）

- 已完成切片 1：`internal path alias` helper 抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/internal_alias.rs`，
  并补 2 个单元测试。
- 已完成切片 2：`strip_explain_prefix` 抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/explain.rs`，
  并补 3 个单元测试。
- 已完成切片 3：删除/创建校验簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/write_validation.rs`，
  包含 `validate_delete_expression / contains_delete_label_predicate / delete_expression_may_yield_entity / validate_create_property_vars`。
- `query_api.rs` 行数从 `3507` 降到 `3348`，调用入口保持不变，仅改为模块导入。
- 切片 3 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test t304_remove_clause` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
  - `cargo test -p nervusdb --test t333_varlen_direction` 通过。
- 已完成切片 4：写入计划编译簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/write_compile.rs`，
  包含 `compile_set_plan_v2 / compile_remove_plan_v2 / compile_unwind_plan / compile_delete_plan_v2`。
- `query_api.rs` 行数从 `3348` 进一步降到 `3215`，仅保留调用入口并通过 `use write_compile::{...}` 分派。
- 切片 4 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test t304_remove_clause` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
  - `cargo test -p nervusdb --test t333_varlen_direction` 通过。
- 已完成切片 5：`CREATE/MERGE` 计划编译簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/write_create_merge.rs`，
  包含 `compile_create_plan / compile_merge_plan`，并保留 `CREATE` 对 `Undirected` 关系的拒绝分支（`RequiresDirectedRelationship`）。
- 已完成切片 6：`MATCH` 重锚定/可选解绑辅助簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/match_anchor.rs`，
  包含 `maybe_reanchor_pattern / first_relationship_is_bound / build_optional_unbind_aliases` 及私有 helper。
- 已完成切片 7：模式谓词防护簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/pattern_predicate.rs`，
  包含 `ensure_no_pattern_predicate / contains_pattern_predicate`。
- 已完成切片 8：`MERGE` set-item 辅助簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/merge_set.rs`，
  包含 `extract_merge_pattern_vars / compile_merge_set_items`。
- 已完成切片 9：表达式类型校验簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/type_validation.rs`，
  包含 `validate_expression_types / is_definitely_non_boolean / is_definitely_non_list_literal`。
- 已完成切片 10：`WHERE` 绑定与作用域校验簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/where_validation.rs`，
  包含 `validate_where_expression_bindings / validate_where_expression_variables / validate_pattern_predicate_bindings` 及局部作用域 helper。
- `query_api.rs` 行数从 `3215` 进一步降到 `2271`，核心编译流程继续收敛为主干调度与调用。
- 切片 5/6 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test merge_test` 通过。
  - `cargo test -p nervusdb --test t323_merge_semantics` 通过。
  - `cargo test -p nervusdb --test t151_optional_match` 通过。
  - `cargo test -p nervusdb --test t333_varlen_direction` 通过。
  - `cargo test -p nervusdb --test t304_remove_clause` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 切片 7 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test t304_remove_clause` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 切片 8 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test merge_test` 通过。
  - `cargo test -p nervusdb --test t323_merge_semantics` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 切片 9 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test t332_binding_validation` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 切片 10 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t332_binding_validation` 通过。
  - `cargo test -p nervusdb --test t151_optional_match` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
- 已完成切片 11：`WITH/RETURN` 计划编译簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/return_with.rs`，
  包含 `compile_with_plan / compile_return_plan`，主流程改为 `use return_with::{...}` 调度。
- `query_api.rs` 行数从 `2271` 进一步降到 `2125`。
- 切片 11 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
  - `cargo test -p nervusdb --test t332_binding_validation` 通过。
  - `cargo test -p nervusdb --test t151_optional_match` 通过。
- 阶段门禁（三件套）补跑结果（2026-02-12）：
  - `bash scripts/workspace_quick_test.sh` 通过。
  - `bash scripts/contract_smoke.sh` 通过。
  - `bash scripts/binding_smoke.sh` 通过（仅保留既有 `gil-refs` 警告，无失败）。
- 已完成切片 12：投影/聚合/排序编译簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/projection_compile.rs`，
  包含 `compile_projection_aggregation / validate_order_by_scope / rewrite_order_expression / contains_aggregate_expression / compile_order_by_items` 及关联私有 helper。
- `query_api.rs` 行数从 `2125` 进一步降到 `1406`。
- 切片 12 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
  - `cargo test -p nervusdb --test t313_functions` 通过。
  - `cargo test -p nervusdb --test t62_order_by_skip_test` 通过。
  - `cargo test -p nervusdb --test t332_binding_validation` 通过。
- 已完成切片 13：`MATCH` 计划编译簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/match_compile.rs`，
  包含 `compile_match_plan` 主入口与 `compile_pattern_chain`、`is_bound_before_local`、
  `apply_filters_for_alias`、`extend_predicates_from_properties` 等内部 helper。
- `query_api.rs` 行数从 `1406` 进一步降到 `984`。
- 切片 13 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t333_varlen_direction` 通过。
  - `cargo test -p nervusdb --test t151_optional_match` 通过。
  - `cargo test -p nervusdb --test t318_paths` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 已完成切片 14：变量绑定推断/校验簇抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/binding_analysis.rs`，
  包含 `validate_match_pattern_bindings / infer_expression_binding_kind / extract_output_var_kinds`，
  以及 `variable_already_bound_error` 等关联 helper。
- `query_api.rs` 行数从 `984` 进一步降到 `651`。
- 切片 14 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t332_binding_validation` 通过。
  - `cargo test -p nervusdb --test t151_optional_match` 通过。
  - `cargo test -p nervusdb --test t324_foreach` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 阶段门禁（三件套）复跑结果（2026-02-12）：
  - `bash scripts/workspace_quick_test.sh` 通过。
  - `bash scripts/contract_smoke.sh` 通过。
  - `bash scripts/binding_smoke.sh` 通过（仅保留既有 `gil-refs` 警告，无失败）。
- 已完成切片 15：`FOREACH` 子计划编译抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/foreach_compile.rs`，
  保持 `compile_foreach_plan` 调用点与行为不变，仅改为模块分发。
- `query_api.rs` 行数从 `651` 进一步降到 `616`。
- 切片 15 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t324_foreach` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 已完成切片 16：写路径判定 helper 抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/plan_introspection.rs`，
  包含 `plan_contains_write`，`execute_mixed` 调用链保持不变。
- `query_api.rs` 行数从 `616` 进一步降到 `580`。
- 切片 16 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test t323_merge_semantics` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 已完成切片 17：主编译管线抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/compile_core.rs`，
  包含 `CompiledQuery` 与 `compile_m3_plan`，并保持子查询/UNION/FOREACH 递归编译行为不变。
- `query_api.rs` 行数从 `580` 进一步降到 `341`。
- 切片 17 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t319_subquery` 通过。
  - `cargo test -p nervusdb --test t324_foreach` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 阶段门禁（三件套）复跑结果（2026-02-12，切片 17 后）：
  - `bash scripts/workspace_quick_test.sh` 通过。
  - `bash scripts/contract_smoke.sh` 通过。
  - `bash scripts/binding_smoke.sh` 通过（仅保留既有 `gil-refs` 警告，无失败）。
- 已完成切片 18：`prepare` 入口实现抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/prepare_entry.rs`，
  `query_api.rs` 保留公开 `prepare` 壳函数并委托新模块，EXPLAIN/普通路径行为保持不变。
- `query_api.rs` 行数从 `341` 进一步降到 `306`。
- 切片 18 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t52_query_api` 通过。
  - `cargo test -p nervusdb --test t104_explain_test` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
- 已完成切片 19：`PreparedQuery` 实现体抽取到
  `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-query/src/query_api/prepared_query_impl.rs`，
  `query_api.rs` 保留结构定义与公开入口，执行实现改为模块化。
- `query_api.rs` 行数从 `306` 进一步降到 `140`。
- 切片 19 回归结果：
  - `cargo test -p nervusdb-query --lib` 通过。
  - `cargo test -p nervusdb --test t52_query_api` 通过。
  - `cargo test -p nervusdb --test t104_explain_test` 通过。
  - `cargo test -p nervusdb --test create_test` 通过。
  - `cargo test -p nervusdb --test t311_expressions` 通过。
