# T38: Node 真流式 Statement + 契约门禁（对齐 `nervusdb.h`）

## 0. Linus 三问（先把脑子摆正）

1) **这是真问题还是臆想？**  
真问题。当前 Node 的 `prepareV2()` 在 Rust 侧把结果全部 `collect` 成 `Vec<Vec<Value>>`，然后 `step()` 只是移动索引。这是**伪流式**：大结果集照样 OOM，只是把对象爆炸从 V8 挪到 Rust 堆里。

2) **有更简单的办法吗？**  
有。别再造第二套“Statement 生命周期”，直接复用 `nervusdb-core` 已经存在的 `PhysicalPlan::execute_streaming`（T17 已做）。每次 `step()` 拉一行，缓存当前行，`column_*` 从当前行取值。就这么简单。

3) **会不会破坏现有东西？**  
不应该。保持 Node public API（TS 层 `CypherStatement`、native 层 `StatementHandle` 方法名）不变，只改变内部实现：从“预加载 rows”变成“惰性 iterator”。行为上仍是 `prepareV2 -> step -> column_* -> finalize`。

## 1. Context

唯一硬契约在 `nervusdb-core/include/nervusdb.h`：

- `nervusdb_prepare_v2` / `nervusdb_step` / `nervusdb_column_*` / `nervusdb_finalize`
- `NERVUSDB_VALUE_{NULL,TEXT,FLOAT,BOOL,NODE,RELATIONSHIP}`

Node 端目前的 Statement API 名义上对齐，但实现上仍在 `prepareV2()` 阶段把所有结果 collect，违反“真流式”目标。

## 2. Goals

- **真流式**：Node 侧 `prepareV2()` 不再预加载全量 rows；`step()` 每次只取下一行。
- **契约门禁**：CI 里加一个“结构级”检查，保证 Node 侧暴露的 Statement API 没有漂移（方法名/数量/ValueType 常量映射）。
- **零破坏**：不改 `nervusdb.h`，不改 Node public API 签名；只允许新增 *可选* 能力（例如更严格的 contract check 脚本），不要求用户改代码。

## 3. Non-Goals

- 不在 T38 做跨语言“行为级 contract test”（Node vs C ABI 逐行逐列对比）。那是更硬的门禁，但会把 CI 复杂度拉满；留到后续任务（可选 T40）。
- 不承诺查询的 snapshot/事务隔离语义（Node 在迭代过程中穿插写入会怎样）。本任务只修“内存不线性爆炸”。

## 4. Solution

### 4.1 数据结构（关键点）

把 `StatementInner` 从：

- `rows: Vec<Vec<CoreValue>>` + `next_row/current_row`

改为：

- `iter: Box<dyn Iterator<Item = Result<Record, Error>> + 'static>`（来自 `PhysicalPlan::execute_streaming`）
- `columns: Vec<String>`
- `current_row: Option<Vec<CoreValue>>`（仅缓存当前行，供 `column_*` 读取）

### 4.2 执行流程（消灭 special case）

1) `prepareV2(query, params)`：
   - 解析 query（`Parser::parse`），抽取 RETURN projection 名字作为 `columns`（与 C ABI 一致）。
   - params 转 `HashMap<String, executor::Value>`（复用 `Database::serde_value_to_executor_value`）。
   - 生成 `PhysicalPlan`（`QueryPlanner::plan`）。
   - 构建 `ArcExecutionContext`，调用 `plan.execute_streaming(ctx)` 得到 iterator。
   - 返回 `StatementHandle { inner: ... }`，此时不产生任何结果集分配。

2) `step()`：
   - 从 iterator 取下一条记录；若 `Ok(record)`，把 record 按 `columns` 投影成 `Vec<CoreValue>` 存入 `current_row`，返回 `true`。
   - iterator 结束返回 `false`。
   - iterator 返回 `Err(e)`，向 JS 抛错（NapiError）。

3) `column_*()`：
   - 只读取 `current_row`，不再触碰数据库。

### 4.3 契约门禁（CI）

新增 `bindings/node/scripts/check-contract.mjs`：

- 读取 `nervusdb-core/include/nervusdb.h`，用最小正则抽出：
  - `NERVUSDB_VALUE_*` 常量集合
  - statement 函数集合（至少 `prepare_v2/step/column_{count,name,type,text,bytes,double,bool,node_id,relationship}/finalize`）
- 读取 `bindings/node/native/nervusdb-node/npm/index.d.ts`，断言：
  - `StatementHandle` 暴露的方法集合与预期一致（允许 `columnDouble` 映射为 `columnFloat`，但必须明确写在脚本里）
  - `DatabaseHandle.prepareV2(query, params)` 存在

脚本要**失败就 fail CI**，别搞“warning-only”这种自欺欺人的玩意。

## 5. Testing Strategy

- Rust：`cargo test -p nervusdb-node`（现有 node-native 测试继续跑）。
- Node：跑现有 `pnpm -C bindings/node test:native`（覆盖 statement 基本行为）。
- 新增：`pnpm -C bindings/node check:contract`（只做静态契约检查，快、稳定）。

## 6. Risks

- **并发/互斥**：iterator 里会访问数据库；需要确保 Node 侧不会在多线程并发调用同一个 DB 句柄。当前 N-API binding 已通过 mutex 保护大部分调用，但 statement iterator 的访问路径必须同样受控。
- **列名推断**：没有 RETURN projection 的查询，columns 可能为空；与 C ABI 行为保持一致即可（不在 T38 解决“无 RETURN 的列集合推断”）。

