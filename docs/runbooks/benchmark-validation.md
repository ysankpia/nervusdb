# Benchmark Validation Runbook

Benchmarks are evidence, not a daily tax. Use them when performance is the
point of the change or when preparing 0.1 readiness evidence.

## Small Benchmark

```bash
bash scripts/core_bench.sh --small
```

Use this for local sanity checks after storage, traversal, or query-path changes.

## Large Benchmark

```bash
bash scripts/core_bench.sh --large
```

Large 1,000,000 node / 5,000,000 edge runs are manual. They do not belong in the
default CI loop.

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
