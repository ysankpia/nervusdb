# T324: FOREACH Clause

## 1. 概述

实现 Cypher 的 `FOREACH` 子句，用于对列表中的每个元素执行更新操作。
语法：`FOREACH ( variable IN listExpression | updatingClauses )`

常用于：

- 批量创建节点/关系
- 根据列表条件更新属性

## 2. 需求与约束

### 2.1 语法

支持标准形式：

```cypher
FOREACH ( x IN [1, 2, 3] | CREATE (:Node {id: x}) )
```

- `variable`: 循环变量名
- `listExpression`: 任意求值为 List 的表达式
- `|`: 分隔符
- `updatingClauses`: 允许由 `CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`, `FOREACH` 组成的序列。
  - **限制**: 不允许 `MATCH`, `RETURN`, `WITH`, `UNWIND`, `CALL` 等读取/投影子句。

### 2.2 行为

- 输入流的每一行都会触发一次 `FOREACH` 执行。
- 对于输入行，计算 `listExpression`。
- 遍历列表，将元素绑定到 `variable`。
- 对每个元素执行 `updatingClauses`。
- `FOREACH` 执行完毕后，**透传** 原始输入行到下一个子句（cardinality 不变）。
- 返回值：`FOREACH` 本身不产生新行，只产生 side effects。

## 3. 设计方案

### 3.1 AST (`nervusdb-v2-query/src/ast.rs`)

新增 `Clause::Foreach`:

```rust
pub enum Clause {
    // ...
    Foreach(ForeachClause),
}

pub struct ForeachClause {
    pub variable: String,
    pub list: Expression,
    pub updates: Vec<Clause>, // 限制为 updating clauses
}
```

### 3.2 Parser (`nervusdb-v2-query/src/parser.rs`)

- 新增 `parse_foreach()`
- 识别 `FOREACH` -> `(` -> variable -> `IN` -> expr -> `|` -> clauses -> `)`
- 需要复用现有的 `parse_clause` 但限制允许的类型，或者递归调用通用 parse 但后校验。
- 鉴于 `updatingClauses` 可以包含多个子句（如 `CREATE ... SET ...`），parser 需要能解析子句列表直到遇到 `)`。

### 3.3 Planner (`nervusdb-v2-query/src/query_api.rs`)

- 扩展 `Plan` enum：

```rust
pub enum Plan {
    // ...
    Foreach {
        list: Expression,
        variable: String,
        sub_plan: Box<Plan>, // 内部的更新链
        next: Box<Plan>,     // FOREACH 之后的后续操作
    }
}
```

- 编译时，将 `updates` 列表编译为 `sub_plan` 链。

### 3.4 Executor (`nervusdb-v2-query/src/executor.rs`)

- 实现 `execute_plan` for `Plan::Foreach`.
- 逻辑：
  ```rust
  Plan::Foreach { list, variable, sub_plan, next } => {
      let input_iter = execute_plan(snapshot, input, params); // Note: Foreach usually wraps input?
      // Actually Foreach is a clause, so it will likely be chained.
      // If Plan matches `Foreach { input, ... }` structure:

      let iter = input_iter.map(|row_result| {
          let row = row_result?;
          let list_val = evaluate(list, &row, ...)?;
          if let Value::List(items) = list_val {
              for item in items {
                   let mut sub_row = row.clone().with(variable, item);
                   // Execute sub_plan for side effects only
                   // Use a special execute_write_subplan or similar that drains the result?
                   // However, our executor works on iterators.
                   // We need to drive the iterator to completion.
                   let sub_iter = execute_plan(snapshot, sub_plan, &new_params?);
                   for _ in sub_iter {} // Drain
              }
          }
          Ok(row) // Pass through original row
      });
      // Then execute `next` on this iterator?
      // Or `Plan` structure usually encapsulates the `next` step in the recursion or via chaining.
      // In T105/T207, `Plan` is often "Operation { input: Box<Plan> }".
      // So Foreach should be `Plan::Foreach { input: Box<Plan>, ... }`.
  }
  ```
- **Important**: The `updatingClauses` inside FOREACH are themselves a query plan (e.g. `Create -> Set`). This sub-plan usually terminates (doesn't Return).

## 4. 测试计划

### 4.1 单元测试

- Parser tests: 嵌套 FOREACH, 多个 Update 子句。

### 4.2 集成测试 (`tests/t324_foreach.rs`)

1. **Basic Create**:
   `FOREACH (x IN [1,2] | CREATE (:A {val: x}))`
   Verify: 2 nodes created.

2. **Scoped Update**:
   `MATCH (n:User) WHERE n.id = 1 FOREACH (name IN ['Alias'] | SET n.alias = name)`
   Verify: Update happens.

3. **Nested FOREACH**:
   `FOREACH (x IN [1] | FOREACH (y IN [2] | CREATE (:N {sum: x+y})))`

4. **Empty List**:
   `FOREACH (x IN [] | CREATE (:ShouldNotExits))`
   Verify: 0 created.
