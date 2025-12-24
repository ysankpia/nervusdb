# T37: UniFFI 多语言绑定（以 C ABI Statement 为唯一硬契约）

## 1. Context

NervusDB 已经通过 T6/T10/T13 冻结并验证了最小稳定跨语言契约：`nervusdb-core/include/nervusdb.h` 的 C ABI，核心查询模型为 SQLite-style Statement：

- `prepare_v2 → step → column_* → finalize`
- `ValueType`/`Relationship` 的返回语义已在 C/Node 端落地并验证

当前问题很简单也很致命：Python 绑定仍走 `pyo3` + 一次性物化（`execute_query() -> list[dict]`），与“真流式”目标相反，并且多语言扩展会带来接口漂移风险。

本任务目标：以 **C ABI + Statement 语义**为唯一硬契约，引入 UniFFI 作为“复制语义到更多语言”的生成器，重做 Python 绑定，并为 Swift/Kotlin/Ruby 等预留低成本扩展路径。

## 2. Goals

- **契约唯一事实来源**：`nervusdb.h` + `ffi.rs` 的 Statement/ValueType 语义（不改任何现有 ABI 签名）
- **Python 绑定彻底重做**：默认提供同步真流式 iterator；允许破坏旧 Python API（用户接近 0）
- **Node 保持不动**：继续使用现有 `napi-rs` 绑定（性能最强），只新增“契约一致性门禁”
- **未来扩展**：Swift/Kotlin/Ruby 使用 UniFFI 自动生成，复刻同一 Statement 语义

## 3. Non-Goals

- 不引入新的对外查询协议（不再发明“另一套 streaming API”）
- 不修改 `nervusdb.h` 现有函数签名（ABI 冻结继续生效）
- Python 暂不提供“真正 async iterator”（后续若需要，仅做薄包装：`asyncio.to_thread`）
- 不在本任务内扩展 C ABI 的 Vector/Integer 等新 ValueType（C ABI 既定：Vector 当前按 TEXT(JSON) 表示）

## 4. Solution

### 4.1 总体架构

```
nervusdb-core
├── include/nervusdb.h         # 唯一硬契约（已冻结）
├── src/ffi.rs                 # C ABI 实现（Statement + column_*）
└── src/lib.rs                 # 执行器/存储/查询引擎

bindings/
├── node/                      # 保留 napi-rs（已对齐 Statement 模型）
├── uniffi/nervusdb-uniffi/    # 新增：UniFFI 导出层（复刻 Statement 语义）
└── python/nervusdb-py/        # 新版：maturin + uniffi 生成 + 极薄 Python 包装
```

关键原则：**UniFFI 不是新的契约**，它只是把 `nervusdb.h` 的 Statement 语义“搬运”到更多语言。

### 4.2 关键数据结构（契约层）

以 `nervusdb-core/src/ffi.rs` 为金标准，UniFFI 导出必须保持同构语义：

- `Database`：打开/关闭 DB；创建 Statement
- `Statement`：`step()` 驱动游标；`column_*()` 读取当前行列值；`finalize()` 释放资源
- `ValueType`：必须至少覆盖现有 C ABI：
  - `NULL/TEXT/FLOAT/BOOL/NODE/RELATIONSHIP`
  - 注意：当前 C ABI 不存在 `VECTOR/INTEGER`，UniFFI 层也不得擅自扩展“新类型语义”
- `Relationship`：`(subject_id, predicate_id, object_id)`（与 `nervusdb_relationship` 同构）

### 4.3 UniFFI 导出策略：避免重复实现 step/column 逻辑

坏味道：在 UniFFI crate 内“从 Rust 调用 C ABI 函数”去复用逻辑。这会把安全性和可维护性打碎。

正确做法：复用 **Rust 侧的 Statement 实现结构**（`nervusdb_core::ffi::CypherStatement / StmtValue / convert_stmt_value` 等），必要时在 `nervusdb-core` 内新增一个不破坏 ABI 的内部 helper，供：

- C ABI `nervusdb_prepare_v2/nervusdb_step/...` 调用
- Node napi-rs Statement 调用
- 新 UniFFI Statement 调用

目标是把“生成一行 current_row”这套逻辑只有一份实现，避免三份分叉。

### 4.4 UniFFI API（概念草案）

UniFFI 层导出接口应当贴近 C ABI 语义，而不是贴近某种语言的“惯用风格”：

- `Database.open(open_options) -> Database`
- `Database.prepare_v2(query: String, params_json: Option<String>) -> Statement`
- `Statement.step() -> bool`（`true` 表示当前行可读，`false` 表示结束）
- `Statement.column_count() -> u32`
- `Statement.column_name(i) -> Option<String>`
- `Statement.column_type(i) -> ValueType`
- `Statement.column_text/column_double/column_bool/column_node_id/column_relationship`
- `Statement.finalize()`（并且绑定层应在 GC/Drop 时兜底调用）

说明：
- `params_json` 保持与 `nervusdb_prepare_v2` 同构：JSON object 字符串（或 `None`）
- TEXT 的语义：与 C ABI 一致；Vector 当前会被表示为 TEXT(JSON string)（不额外创造 Vector 类型）

### 4.5 Python 绑定（同步真流式）

Python 包结构分两层：

1) UniFFI 自动生成的底层模块（提供 `Database/Statement/...`）
2) 极薄手写 Python 包装：
   - 提供 `__iter__/__next__` 将 `step + column_*` 组装成 `dict` 或 `Row` 对象
   - 只做“组装”，不做缓存/物化

推荐 Python 侧 API（示意）：

- `db = Database(path)`
- `stmt = db.prepare("MATCH ...", params_json=None)`
- `for row in stmt:`（每次 `__next__` 读取当前行）

注意：这不是“每行都去构造巨大对象”的借口。若性能敏感，允许用户直接使用 `Statement` 的 `column_*` 接口逐列读取，减少字典分配。

### 4.6 Node 绑定（保持不动 + 门禁）

Node 侧继续保持现有实现（`bindings/node/native/nervusdb-node` + TS 外壳）。新增门禁目标：

- 校验 `CypherValueType`/`Relationship`/`Statement` 方法集合与 C ABI 语义一致
- 校验 Node 公共 TS API 中“Statement 模型相关”的命名/返回语义不漂移

实现方式建议（择一即可）：

- 方式 A：脚本解析 `nervusdb.h` 的 enum/函数列表，和 Node native 导出的 `.d.ts` 做最小子集对比
- 方式 B：直接运行一个“行为级 contract test”：对同一组查询，Node 的 Statement 逐行逐列读取，与 C ABI 驱动读取结果一致（推荐，最不怕签名差异）

## 5. Testing Strategy

### 5.1 生成验证

- UniFFI：确保能生成 Python/Swift/Kotlin 的 binding 产物（至少 Python 作为 P0）
- Python：`maturin build/develop` 后可 import，能跑最小 smoke

### 5.2 行为一致性（核心门禁）

建立一组“跨语言黄金用例”（固定输入数据 + 固定 Cypher）：

- 基础类型：TEXT/FLOAT/BOOL/NULL
- 图类型：NODE/RELATIONSHIP
- 典型语句：`MATCH/OPTIONAL MATCH/WITH/ORDER BY/LIMIT/UNION`（覆盖你们差异最大的执行路径）
- 错误场景：语法错、NotImplemented、invalid params_json

比较方式：把结果归一化为：

- `columns: [name...]`
- `rows: [[typed_value...], ...]`（typed_value 只用 C ABI 语义集合）
- `error: { code, message_prefix }`

Node 与 Python 都要过，且与 C ABI 的结果一致。

### 5.3 性能与内存门禁

- Python 大结果集扫描：确认 RSS 不线性增长（流式消费下）
- 性能基准：Python Statement 逐行读取 vs Rust core（允许 <5% 包装开销目标；以基准数据说话）

## 6. Risks

- **UniFFI 并发模型**：UniFFI 不允许 `&mut self` 暴露，Statement 必须用内部 `Mutex/RwLock` 管理可变状态；需要明确线程安全语义（推荐：实现为线程安全，代价可接受）
- **UDL/接口漂移**：如果手写 UDL（或手写导出 API）而不做门禁，漂移一定发生；必须以行为级 contract test 兜底
- **Python API 破坏**：本任务明确允许 break；但需要在 README/CHANGELOG 写清楚（避免“用户导入就炸”）
- **Vector 表示**：目前 C ABI 只把 Vector 降级成 TEXT(JSON)。UniFFI/Python 也必须一致，否则就是新的特殊情况

## 7. Implementation Plan (Phased)

1) 新增 `bindings/uniffi/nervusdb-uniffi` crate（先只覆盖 Database+Statement+ValueType+Relationship+Error）
2) Python 重做为 “maturin + uniffi bindings + 薄包装”
3) 加入跨语言 contract tests（至少 Node vs Python vs C ABI）
4) 后续按需扩展 temporal/vector/fts 的导出面（严格受 feature gate 控制）

