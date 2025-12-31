# T318 Design Plan: Path Values + Path Functions

## 1. Overview

支持 Cypher 中的路径变量（Path Variables）及其相关函数（`length()`, `nodes()`, `relationships()`）。

## 2. Requirements Analysis

### 2.1 Usage Scenarios

1. `MATCH p = (a)-[r]->(b) RETURN p`
2. `MATCH p = (a)-[r*1..3]->(b) RETURN nodes(p), relationships(p)`
3. `MATCH p = (a)-[r1]->(b)-[r2]->(c) RETURN length(p)`

### 2.2 Functional Requirements

- 解析支持 `p = (pattern)` 语法。
- 执行器支持 `Value::Path` 类型，包含节点 ID 序列和边 Key 序列。
- 计划器能够在匹配过程中同步构建路径值。
- 函数支持：获取路径长度、节点列表和关系列表。

## 3. Test Case Design

### 3.1 Unit Test Cases

- `PathValue` 序列化与反序列化。
- `length()` 对不同长度路径的处理。

### 3.2 Integration Test Cases

- **基础路径赋值**：`MATCH p = (a:Person {name:'Alice'})-[:KNOWS]->(b) RETURN p`
- **变长路径赋值**：`MATCH p = (a)-[*1..2]->(b) RETURN length(p)`
- **路径节点与关系提取**：`MATCH p = (a)-[]->(b)-[]->(c) RETURN nodes(p), relationships(p)`

## 4. Design Scheme

### 4.1 Core Data Structure

在 `nervusdb-v2-query/src/executor.rs` 中：

```rust
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct PathValue {
    pub nodes: Vec<InternalNodeId>,
    pub edges: Vec<EdgeKey>,
}

pub enum Value {
    ...
    Path(PathValue),
}
```

### 4.2 AST Change

在 `ast.rs` 中：

```rust
pub struct Pattern {
    pub variable: Option<String>, // 用于路径赋值
    pub elements: Vec<PathElement>,
}
```

### 4.3 Parser Change

修改 `parser.rs` 的 `parse_pattern`，检查 `identifier =`。

### 4.4 Executor & Planner Change

- `Plan` 相关变体（`MatchOut`, `MatchIn`, `MatchUndirected`, `MatchOutVarLen`）增加 `path_alias: Option<String>`。
- 执行器在每一跳时，如果 `path_alias` 存在，则更新 `Value::Path`。
  - 如果 `p` 不存在，初始化为 `PathValue { nodes: [src, dst], edges: [edge] }`。
  - 如果 `p` 已存在，追加 `dst` 到 `nodes`，追加 `edge` 到 `edges`。

## 5. Implementation Plan

### Step 1: Data Types & AST

- 修改 `executor.rs` 添加 `PathValue` 和 `Value::Path`。
- 修改 `ast.rs` 为 `Pattern` 添加 `variable`。

### Step 2: Parser

- 更新 `parse_pattern` 以支持路径变量赋值。

### Step 3: Planner & Executor Logic

- 更新 `Plan` 枚举及相关构造逻辑。
- 在 `execute_plan` 对应的迭代器中实现 `Path` 的实时构建。

### Step 4: Path Functions

- 在 `evaluator.rs` 中实现 `length`, `nodes`, `relationships` 函数。
