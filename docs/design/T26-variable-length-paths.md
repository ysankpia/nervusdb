# T26: Cypher Variable-Length Paths

## 1. Context

Cypher patterns needed variable-length traversal (e.g., *1..3).

## 2. Goals

- Support relationship patterns with variable-length ranges.
- Respect min/max hop bounds.

Non-Goals

- Relationship variables or property filters on variable-length segments.
- Multiple relationship types in a single variable-length segment.

## 3. Solution

### 3.1 AST

Use RelationshipPattern.variable_length (min/max).

### 3.2 Parser

Parse *min..max, *min.., *..max, and *.

### 3.3 Executor

For variable-length segments, precompute reachable nodes using BFS within the
min/max hop range and expand matches from that set.

## 4. Testing Strategy

- parser::tests::test_parse_variable_length_relationship
- parser::tests::test_execute_multi_hop

## 5. Risks

- BFS-based expansion can be expensive on large graphs.
- Current implementation limits variable-length segments to a single type and
  disallows relationship variables/properties.
