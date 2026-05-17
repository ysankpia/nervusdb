# S3：Bindings 契约回归（Python/Node）

更新时间：2026-02-13  
任务类型：Phase 2  
任务状态：Done

## 1. 目标

- 验证并锁定 Python/Node 的错误分类与 payload 契约。
- 在重构过程中确保 bindings 行为不回退。
- 将契约回归固定为每个后续 PR 的执行项。

## 2. 边界

- 允许：补测试、补断言、补文档。
- 禁止：重定义错误模型、改外部类型语义。
- 禁止：引入不兼容 payload 字段变更。

## 3. 文件清单

### 3.1 必查/必改文件

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/src/lib.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/src/types.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-node/src/lib.rs`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-node/index.d.ts`

### 3.2 测试与脚本

- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/tests/test_basic.py`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/tests/test_vector.py`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/binding_smoke.sh`
- `/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/contract_smoke.sh`

## 4. 前置证据

- spec 中已定义跨语言错误模型与门禁：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:24`；`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/spec.md:42`
- tasks 中 M5-01 为 WIP：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/docs/tasks.md:86`

## 5. 测试清单

1. Python 语义：
   - 异常类型映射：`Syntax/Execution/Storage/Compatibility`
   - 结构化 payload 完整性
2. Node 语义：
   - `code/category/message` 字段稳定
   - 异常路径与成功路径类型稳定
3. 门禁脚本：
   - `bash scripts/binding_smoke.sh`
   - `bash scripts/contract_smoke.sh`

## 6. 回滚步骤

1. 任一语言绑定契约破坏即回滚 PR。
2. 增加契约测试后再重提，不允许只改文档放行。

## 7. 完成定义（DoD）

- Python/Node 契约用例通过。
- 跨语言错误模型与 spec 保持一致。
- 所有相关 PR 的 bindings smoke/contract smoke 全绿。

## 8. 当前进展（2026-02-13）

1. 已完成 Node 侧错误契约加固
   - 文件：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-node/src/lib.rs`。
   - 新增/补强用例：
     - `napi_err_maps_syntax_messages_to_syntax_category`
     - `napi_err_maps_expected_prefix_to_syntax_category`
     - `napi_err_maps_storage_messages_for_fs_failures`
     - `napi_err_falls_back_to_execution_category`
     - `classify_err_message_keeps_compatibility_priority`
   - 修正点：补齐 `permission denied / no such file / disk full / Expected ...` 的分类规则，保持 `code/category/message` payload 稳定。

2. 已完成 Python 侧分类一致性收敛
   - 文件：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/nervusdb-pyo3/src/lib.rs`。
   - 调整 `classify_error_text` 优先级为：
     `Compatibility > Syntax > Storage > Execution`。
   - 新增/补强用例：
     - `classify_maps_syntax_errors`（补 `Expected ...`）
     - `classify_maps_storage_errors`（补 `permission denied / disk full`）
     - `classify_prioritizes_compatibility_when_multiple_keywords_exist`

3. 已完成 contract smoke 脚本契约断言增强
   - 文件：`/Volumes/WorkDrive/Code/github.com/LuQing-Studio/rust/nervusdb/scripts/contract_smoke.sh`。
   - Node runtime 增加错误路径断言：
     - 语法错误必须输出 `NERVUS_SYNTAX/syntax`
     - 关闭数据库后错误必须输出 `NERVUS_STORAGE/storage`
     - payload 必须包含 `code/category/message` 三字段且为字符串

4. 验证结果
   - `cargo test -p nervusdb-pyo3`：Pass
   - `cargo test --manifest-path nervusdb-node/Cargo.toml --lib`：Pass
   - `bash scripts/contract_smoke.sh`：Pass
   - `bash scripts/binding_smoke.sh`：Pass
