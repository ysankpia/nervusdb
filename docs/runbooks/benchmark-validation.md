# Benchmark Validation Runbook

Benchmarks are evidence, not a daily tax. Use them when performance is the
point of the change or when preparing 0.1 readiness evidence.

## Small Benchmark

```bash
bash scripts/core_bench.sh --small
```

Use this for local sanity checks after storage, traversal, or query-path changes.
It writes JSON and log artifacts under `artifacts/core-bench/`, which is ignored
by git.

## Large Benchmark

```bash
bash scripts/core_bench.sh --large
```

Large 1,000,000 node / 5,000,000 edge runs are manual. They do not belong in the
default CI loop.

Use large mode only for 0.1 release-candidate evidence or targeted storage
benchmark work:

```bash
bash scripts/core_bench.sh --large
```

## Record Format

For every meaningful benchmark, record:

- hardware and OS
- command
- git commit
- node and edge count
- query shape
- P50, P95, and P99 when available
- whether the run is comparable to a previous baseline

Do not use benchmark work to justify expanding vector, optimizer, or full-Cypher
scope before 0.1.

## Cross-Database Research

Use `docs/research/embedded-graph-benchmark.md` when comparing NervusDB against
SQLite-as-graph, Kuzu, or other embedded graph/database systems.

Phase 1 harness:

```bash
bash scripts/cross_db_bench.sh --small
bash scripts/cross_db_bench.sh --medium
```

Run one system only when diagnosing a backend:

```bash
bash scripts/cross_db_bench.sh --system sqlite-materialized --small
```

The script writes per-system JSON files and one NDJSON summary under
`artifacts/cross-db-bench/`.

Cross-database results are invalid unless they use the same generated data,
same durability profile, same correctness hash, and clearly separated product
classes. In particular, do not compare SQLite unsafe writes against NervusDB
durable writes, and do not compare Kuzu bulk `COPY FROM` against row-by-row
application writes without labeling the workload difference.
