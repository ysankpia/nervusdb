# T320 Design: Procedure CALL/YIELD

## 1. Overview

Implement the infrastructure for calling built-in and extension procedures in Cypher.
Examples: `CALL db.info() YIELD version`, `CALL vector.search('idx', $vec, 10) YIELD node`.

## 2. Requirements

- Support `CALL namespace.name(args) YIELD c1, c2 AS alias`.
- Execute procedures either standalone or as part of a query pipeline (per-row execution).
- Extensible registry for future modules (like vector search).

## 3. Test Cases

- `CALL math.add(1, 2) YIELD result`: Simple arithmetic procedure.
- `MATCH (n:Person) CALL db.node_labels(n) YIELD labels`: Correlated procedure call (runs for each node).

## 4. Design Scheme

### 4.1 AST Changes

`ast.rs`:

```rust
pub enum CallClause {
    Subquery(Query),
    Procedure(ProcedureCall),
}

pub struct ProcedureCall {
    pub name: Vec<String>,
    pub arguments: Vec<Expression>,
    pub yields: Option<Vec<YieldItem>>,
}
```

### 4.2 Procedure Registry

In `executor.rs`:

```rust
pub trait Procedure: Send + Sync {
    fn execute(&self, snapshot: &dyn GraphSnapshot, args: Vec<Value>) -> Result<Vec<Row>>;
}

pub struct ProcedureRegistry {
    handlers: HashMap<String, Box<dyn Procedure>>,
}
```

_Note: Using Vec<Row> for MVP simplicity in materialization, similar to how Apply handles subqueries._

### 4.3 Execution Semantics

- If `CALL` is the first clause, it executes once against a single dummy row (`Plan::ReturnOne`).
- If `CALL` follows other clauses, it executes for each incoming row (Apply logic).
- Output variables from `YIELD` are joined to the incoming row.

## 5. Implementation Plan

### Step 1: AST & Parser Refactor

1. Update `ast.rs`: Change `CallClause` to enum.
2. Update `parser.rs`: `parse_call` to support procedure syntax + YIELD.

### Step 2: Executor Infrastructure

1. Define `Procedure` trait and `ProcedureRegistry`.
2. Implement `Plan::ProcedureCall` and `ProcedureCallIter`.
3. Register dummy procedures: `db.info`, `math.add`.

### Step 3: Planner Integration

1. Update `compile_m3_plan` to distinguish between subquery and procedure.
2. Implement column mapping for `YIELD`.

### Step 4: Verification

1. Integration tests in `tests/t320_procedures.rs`.
