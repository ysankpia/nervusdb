# NervusDB v2.0 Cypher Compatibility Specification

> **Status**: Draft (v0.1)
> **Target**: openCypher v9 (Partial Compliance)
> **Gate**: `tck_harness` running `Literals`, `Mathematical`, `Comparison` suites.

## 1. Compliance Statement

NervusDB v2.0 implements a **strict subset** of openCypher v9.
We prioritize **correctness over completeness**: features that are implemented must pass relevant TCK tests. Features not implemented will return explicit syntax errors (Fail Fast).

## 2. Supported Features (Exec-Complete)

The following features are tested and considered production-ready:

### Clauses

- **Read**: `MATCH`, `OPTIONAL MATCH`, `RETURN`, `WITH`, `UNWIND`, `UNION`, `UNION ALL`.
- **Write**: `CREATE`, `DELETE`, `DETACH DELETE`, `SET` (Properties & Labels), `REMOVE` (Properties & Labels), `MERGE`.
- **Subqueries**: `CALL { ... }`, `EXISTS { ... }`.
- **Procedures**: `CALL ... YIELD ...`.

### Expressions

- **Literals**: Integer, Float, String, Boolean, Null, List `[]`, Map `{}`.
- **Arithmetic**: `+`, `-`, `*`, `/`, `%`, `^`.
- **Comparison**: `=`, `<>`, `<`, `>`, `<=`, `>=`.
- **Logic**: `AND`, `OR`, `NOT`, `XOR`.
- **String**: `STARTS WITH`, `ENDS WITH`, `CONTAINS`.
- **List**: `IN`, `[]` (indexing), `[..]` (slicing).
- **Control Flow**: `CASE WHEN ... ELSE ... END`.

### Functions

- **Scalar**: `id()`, `type()`, `labels()`, `head()`, `last()`, `size()`, `coalesce()`.
- **Aggregating**: `count()`, `collect()`, `min()`, `max()`, `sum()`, `avg()`.

## 3. Known Limitations (Exclusions)

The following features are **explicitly excluded** from v2.0 scope.

### Syntax & Semantics

1.  **Implicit Grouping Keys**: In `RETURN n, count(*)`, `n` is auto-grouped. Complex expressions in projection like `RETURN n.age + 1, count(*)` are supported, but ordering semantics might differ from Neo4j in edge cases.
2.  **Pattern Comprehension**: `[ (a)-->(b) | b.name ]` is **NOT** supported. Use `CALL { MATCH ... RETURN ... }` instead.
3.  **List Comprehension**: `[x IN list WHERE ... | ...]` is **NOT** supported. Use `UNWIND` + `collect()`.
4.  **Regular Expressions**: `=~` operator is **NOT** supported.
5.  **Path Variables in WRITE**: `CREATE p=(n)-[r]->(m)` path binding is limited.
6.  **Complex Constraints**: `CREATE CONSTRAINT` is not supported (Index creation is explicit via `nervusdb-cli` or `CALL db.createIndex`).
7.  **Legacy Index Hints**: `USING INDEX ...` hints are ignored.

### TCK Specifics

- **Error Codes**: NervusDB returns generic `SyntaxError` or `execution error`, not specific TCK error codes (e.g., `SyntaxError: InvalidArgumentType`).
- **Floating Point**: Precision matches standard IEEE 754, but formatting in error messages may differ.
- **Sorting**: `ORDER BY` on mixed types (e.g., String vs Int) follows simplistic rules (Type order: Null < Bool < Number < String < Map < Base types).

## 4. Verification Gate (Golden Set)

The CI process uses `tests/tck_harness.rs` to enforce compliance.
Current Passing Suites:

- `features/expressions/literals/*`
- `features/expressions/mathematical/*` (target)
- `features/expressions/comparison/*` (target)

Any regression in these suites blocks release.
