# T10：C API 二进制 Row 迭代器 + ABI 冻结策略

## 1. Context

当前 `nervusdb_exec_cypher()` 通过 `char** out_json` 返回整批 JSON 结果。

这玩意儿“能用”，但它有两个系统级问题：

1. **性能浪费**：JSON 文本序列化 + 分配 + 拷贝，都是纯开销。
2. **契约不专业**：你让 C 调用者靠 JSON 解析来拿结果，这是“把数据库当 Web API”。

我们要的是 SQLite 味道：`prepare → step → column* → finalize`，并且**不破坏现有接口**。

## 2. Goals

- 保留并冻结现有 C ABI：
  - `nervusdb_exec_cypher()` 继续存在（兼容旧用户/旧绑定）。
  - 既有符号/签名不改，不玩破坏性重命名。
- 新增一个**最小**的 statement/row 迭代器 API：
  - 允许 C 调用者逐行获取结果，不需要整批 JSON。
  - 值读取以 `column_*` 函数完成，返回指针/标量，**不要求调用方 free**。
- 输出 schema 稳定：
  - 列顺序 = `RETURN` 投影顺序（与核心 planner 逻辑一致）。
  - 若某行缺列，按 `NULL` 处理（消灭“每行 schema 不同”的特殊情况）。
- 为 1.0 做“硬契约”准备：
  - 增加 ABI 版本号（宏或函数），方便绑定层探测能力。

## 3. Solution

### 3.1 新增类型（C 侧）

- `nervusdb_stmt`：不透明指针（opaque）。
- `nervusdb_value_type`：列值类型枚举（`NULL/STRING/FLOAT/BOOLEAN/NODE/RELATIONSHIP`）。
- `nervusdb_relationship`：三元组结构体（`subject_id/predicate_id/object_id`）。

### 3.2 新增 API（C 侧）

最小集合（SQLite 风格，但不搞 bind/prepare 分家，先务实）：

1. `nervusdb_prepare_v2(db, query, params_json, out_stmt, out_error)`
2. `nervusdb_step(stmt, out_error)`：返回 `NERVUSDB_ROW / NERVUSDB_DONE / error`
3. `nervusdb_column_count(stmt)`
4. `nervusdb_column_name(stmt, index)`（`const char*`，NUL 结尾，直到 `finalize`）
5. `nervusdb_column_type(stmt, index)`（`NULL/TEXT/FLOAT/BOOL/NODE/RELATIONSHIP`）
6. `nervusdb_column_*`（按类型取值，SQLite 味道）
   - `nervusdb_column_text(stmt, index)` + `nervusdb_column_bytes(stmt, index)`
   - `nervusdb_column_double(stmt, index)`
   - `nervusdb_column_bool(stmt, index)`
   - `nervusdb_column_node_id(stmt, index)`
   - `nervusdb_column_relationship(stmt, index)`
7. `nervusdb_finalize(stmt)`

### 3.3 指针生命周期（关键契约）

- `nervusdb_column_name()`：**有效期直到 `nervusdb_finalize()`**。
- `nervusdb_column_text()`：**有效期直到下一次 `nervusdb_step()` 或 `nervusdb_finalize()`**。
- 调用方 **不得** 对上述指针调用 `nervusdb_free_string()`（避免 double-free）。

### 3.4 Rust 实现策略（先“蠢”，再“快”）

第一版实现以稳定为先：

- `prepare` 阶段直接复用现有 `Database::execute_query_with_params()` 得到结果集（当前是 eager 收集）。
- 从解析后的 `RETURN` 投影列表构建固定列名数组（与 planner 的 alias 推导一致）。
- 将每行结果按列名对齐为 `Vec<Value>`，缺失填 `Null`。
- `step()` 只移动游标索引；`column_*()` 读取当前行的 `Value` 并输出。

后续如果要把执行改成真正 streaming（不收集全量），也能在 **不改 ABI** 的前提下替换实现。

### 3.5 ABI 冻结策略（1.0 起生效）

- `nervusdb.h`：
  - 1.0 起 **三个月内不改任何既有声明**。
  - 新能力只允许“追加新函数/新枚举值”，不允许改/删/重排结构体字段。
- 增加 `NERVUSDB_ABI_VERSION` + `nervusdb_abi_version()` 用于绑定层 capability 探测。

## 4. Testing Strategy

- Rust 单测（`nervusdb-core/src/ffi.rs` 或 `nervusdb-core/tests/*`）：
  - `prepare/step/finalize` 基本路径
  - 列名顺序 = `RETURN` 顺序
  - `NULL` 补齐规则（某列缺失时）
  - 类型读取：string/float/bool/node/relationship
- 回归：现有 `nervusdb_exec_cypher` JSON 路径不变（兼容测试继续跑）。
- Fuck-off test：
  - CI 保留 `iterations=5` smoke
  - 本地/夜间跑 `iterations=1000`，作为 1.0 gate

## 5. Risks

- **内存**：第一版仍是 eager 收集，超大结果集会占内存（但不比 JSON 路径更差；至少不再额外生成 JSON 文本）。
- **列名冲突**：`RETURN` 未显式 alias、且表达式推导出重复 `col` 时可能冲突。
  - 处理：在 `prepare` 阶段检测重复列名并报错（强迫用户写 alias，消灭特殊情况）。
- **调用顺序错误**：未 `step()` 就调用 `column_*()`。
  - 处理：明确报错（`NERVUSDB_ERR_INVALID_ARGUMENT`），不搞 UB。
