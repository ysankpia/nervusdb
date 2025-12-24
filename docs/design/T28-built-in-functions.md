# T28: Cypher Built-in Functions

## 1. Context

Basic built-in functions are required for practical Cypher queries.

## 2. Goals

- Provide core built-ins for identifiers, labels, and strings.
- Keep function evaluation side-effect free.

Non-Goals

- Full Neo4j/APOC function coverage.
- Custom user-defined functions.

## 3. Solution

### 3.1 Expression Evaluation

Implement function calls in the expression evaluator with a small, explicit
whitelist:

- id(n)
- type(r)
- labels(n)
- keys(n|r)
- size(str)
- toUpper(str)
- toLower(str)
- trim(str)
- coalesce(a, b, ...)

## 4. Testing Strategy

- cypher_query_test::test_function_type
- cypher_query_test::test_function_labels_and_keys

## 5. Risks

- Type mismatches must return Null, not panic.
- Function list should stay small to avoid scope creep.
