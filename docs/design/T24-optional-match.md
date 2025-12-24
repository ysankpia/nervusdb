# T24: Cypher OPTIONAL MATCH

## 1. Context

Optional pattern matching is required for left-join semantics.

## 2. Goals

- Support OPTIONAL MATCH in the clause pipeline.
- Return rows with Nulls when the optional pattern does not match.

Non-Goals

- Advanced optimizer rules for optional chains.

## 3. Solution

### 3.1 AST

Reuse MatchClause.optional to mark OPTIONAL MATCH.

### 3.2 Planner

Plan OPTIONAL MATCH as a LeftOuterJoin with right-side aliases filled as Null
when no match exists.

### 3.3 Executor

LeftOuterJoin preserves left rows and injects Nulls for right aliases.

## 4. Testing Strategy

- parser::tests::test_parse_optional_match
- parser::tests::test_execute_optional_match

## 5. Risks

- WHERE placement after OPTIONAL MATCH can easily change semantics.
- Cardinality explosions if optional patterns are too broad.
