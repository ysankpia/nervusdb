# T31: List Literals and List Comprehensions

## 1. Context

List values are required for IN predicates, RETURN projections, and
comprehension-style transformations.

## 2. Goals

- Support list literals: [1, 2, 3].
- Support list comprehensions: [x IN list WHERE <cond> | <expr>].

Non-Goals

- Full list algebra (reduce, map, filter functions).

## 3. Solution

### 3.1 AST

Add Expression::List and Expression::ListComprehension.

### 3.2 Executor

- List literals evaluate to JSON array strings.
- List comprehensions iterate the source list, apply optional WHERE, and map
  each element to the output list.

## 4. Testing Strategy

- cypher_query_test::test_list_literal_return
- cypher_query_test::test_list_comprehension_filters_and_maps

## 5. Risks

- List values are serialized as JSON strings; downstream consumers must
  preserve that contract.
- Large lists can amplify memory usage.
