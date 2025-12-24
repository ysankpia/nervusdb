# T29: Cypher CASE WHEN

## 1. Context

Conditional expressions are necessary for non-trivial projections.

## 2. Goals

- Support CASE WHEN <cond> THEN <expr> [WHEN ...] [ELSE <expr>] END.
- Evaluate in the expression engine with short-circuit behavior.

Non-Goals

- Searched CASE vs. simple CASE variants beyond WHEN/THEN/ELSE.

## 3. Solution

### 3.1 AST

Use CaseExpression with alternatives and optional else_expression.

### 3.2 Executor

Evaluate alternatives in order and return the first matching THEN result;
otherwise return ELSE or Null.

## 4. Testing Strategy

- cypher_query_test::test_case_when_expression

## 5. Risks

- Mixed result types may require downstream handling.
- Null conditions must be treated as false.
