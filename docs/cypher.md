# Cypher surface

What the query language accepts, and the constraints the executor relies on. The
mechanism behind each is in [architecture.md §7](architecture.md#7-cypher-execution).

## Grammar

```text
CREATE pat
MERGE pat [ON CREATE SET item [, item ...]] [ON MATCH SET item [, item ...]]
          [RETURN item [, item ...]]
          [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]
UNWIND expr AS var [CREATE pat] [RETURN item [, item ...]]
                        [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]
MATCH pat [, pat ...] [WHERE expr]
      [SET item [, item ...]]
      [DELETE var... | DETACH DELETE var...]
      [CREATE pat]
      [RETURN item [, item ...]]
      [ORDER BY expr [ASC|DESC], ...] [SKIP n] [LIMIT n]

item   := * | var | var.key [AS alias] | FUNC(*) | FUNC(var[.key]) [AS alias]
FUNC   := count | sum | avg | min | max
SET    := var.key = <literal|var.key|var> | var:Label
pat    := (var:Label1:Label2 {k: expr}) -[r:TYPE*min..max {k: expr}]-> (var)
expr   := literal | var | var.key | [expr, ...] | FUNC(...)
```

`EXPLAIN <statement>` reports the plan without executing it.

## Constraints the executor relies on

These are not style preferences. Each one is the reason some input is rejected, and
each is pinned by a test.

### Pattern property values are expressions, but MATCH requires literals

`UNWIND [1,2] AS x CREATE (n {v: x})` has to read `x`, so pattern property values are
parsed as expressions. But `MATCH` and `MERGE` pattern properties are **validated as
literals at parse time**: a pattern is matched before any variable is bound, so an
expression there could never be evaluated. Rejecting it is required — silently
matching nothing is indistinguishable from an empty graph.

### `is_mutating()` decides read-lock vs write-lock, and it is the only decider

`MERGE` is unconditionally mutating, even for a run that writes nothing: whether it
writes is only known after matching, and a shared read lock would let two concurrent
`MERGE`s both find nothing and both create. `UNWIND` is mutating only when it carries
`CREATE`. Adding a statement form means deciding this explicitly.

### `SET` / `DELETE` on a scalar binding is an error

`UNWIND [1] AS x SET x.k = 1` cannot write anything, so it fails rather than
succeeding while doing nothing.

### Statements are atomic

A failed write statement is undone before the error returns, through
`NervusDb::rollback_failed_statement`. Without it a statement that failed on record
500 of 1000 kept the first 499 in the buffer pool, and the next commit flushed them.
Do not add a write entry point that skips this.

### Variable binding discipline

Contexts bind `Binding::Node(u64) | Binding::Edge(u64) | Binding::Value(Value)`.
Never key a context by a raw `u64` alone: node and edge ids share a numbering space,
and the `Value` variant carries `UNWIND` elements, which are not entities.

### Aggregation

`count(*)` counts rows; `count(x)` counts non-null bindings. `sum` returns `Int` when
every input is integral, `avg` always returns `Float`, `min`/`max` preserve the input
type. An empty group yields `count = 0`, `sum = 0`, and null for `avg`/`min`/`max`.
`sum` accumulates with `checked_add` and reports overflow rather than losing precision
through `f64` — see the CHANGELOG entry for 2^53.

### Deletion

`DELETE` on a node that still has relationships is a hard error advising
`DETACH DELETE`; `DETACH DELETE` cascades to incident edges and chains the freed slots
into the Freelist.

### In-place node rewrites

`SET n:Label` and similar go through `DiskGraph::update_node_payload`, never
`insert_node_with_id_exact`, which would inflate `node_count`.
