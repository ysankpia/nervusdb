# Temporal Memory Benchmarks

## Overview

The new temporal memory pipeline ships with reproducible micro-benchmarks covering:

- **DMR (Dialogue Memory Retrieval)** — checks that entity mentions remain retrievable via the timeline builder after multi-turn conversations.
- **LongMemEval-inspired timeline checks** — validates `asOf` and `between` filters against staged episodes.

Both benchmarks are intentionally lightweight; they run against sample datasets bundled in the repository (`benchmarks/data/*-sample.json`) so they can execute in CI or locally without external downloads.

## Running the benchmarks

```bash
pnpm bench:temporal
```

This command rebuilds the TypeScript bindings and executes `benchmarks/temporal-memory.mjs`. The script prints two tables:

1. **DMR Sample Accuracy** — recall for each conversation sample. A score of `1` indicates all expected entities were recovered from the timeline.
2. **LongMemEval Sample Checks** — verifies that the timeline contains the expected canonical entities for both `asOf` and `between` queries.

Example output:

```
=== DMR Sample Accuracy ===
┌─────────┬───────────┬──────────┐
│ (index) │ id        │ accuracy │
├─────────┼───────────┼──────────┤
│ 0       │ 'dmr-001' │ 1        │
│ 1       │ 'dmr-002' │ 1        │
└─────────┴───────────┴──────────┘
=== LongMemEval Sample Checks ===
┌─────────┬───────────┬───────────┬─────────────────────────────────────────────────┬─────────────────────────────────────┬──────┐
│ (index) │ id        │ check     │ observed                                        │ expected                            │ ok   │
├─────────┼───────────┼───────────┼─────────────────────────────────────────────────┼─────────────────────────────────────┼──────┤
│ 0       │ 'lme-001' │ 'asOf'    │ [ 'ancient_ruins', 'sapphire_key', 'waterfall' ]│ [ 'ancient_ruins', 'sapphire_key' ] │ true │
│ 1       │ 'lme-001' │ 'between' │ [ 'ancient_ruins', 'sapphire_key', 'archive' ]  │ [ 'sapphire_key', 'archive' ]       │ true │
└─────────┴───────────┴───────────┴─────────────────────────────────────────────────┴─────────────────────────────────────┴──────┘
```

## Extending to full datasets

- Replace `benchmarks/data/dmr-sample.json` and `benchmarks/data/longmemeval-sample.json` with the official benchmark corpora. Maintain the same JSON structure (`conversation/queries` and `episodes/checks`) so the harness continues to work.
- Update expected entity lists in `expectContains` to match the ground-truth answers from the benchmark.
- Commit the updated metrics table (copy from CLI output) to this document to track regressions across releases.

## CI integration

The script is lightweight and can be executed in nightly or release QA workflows. Suggested GitHub Actions snippet:

```yaml
- name: Temporal memory benchmarks
  run: pnpm bench:temporal
```

Because the datasets are bundled with the repository, no external downloads are required. Execution time is under one second on a MacBook M3 Pro.

## Current Status

### v0.6.0 (2025-11-07) - Native Backend

| Benchmark      | Metric            | TypeScript | Native (Rust) | Improvement |
| -------------- | ----------------- | ---------- | ------------- | ----------- |
| DMR            | Recall per sample | 1.0        | 1.0           | Same        |
| LongMem        | asOf / between    | Pass       | Pass          | Same        |
| Timeline Query | Avg latency       | ~5ms       | ~2ms          | **2.5x**    |
| Entity Lookup  | Avg latency       | ~3ms       | ~1ms          | **3x**      |

**Native backend features:**

- ✅ Complete `as_of`/`between` filtering in Rust core
- ✅ Automatic fallback to TypeScript when native unavailable
- ✅ Zero breaking changes - full API compatibility
- ✅ Integration tests verify parity between implementations

See [Native Temporal Migration Guide](../NATIVE_TEMPORAL_MIGRATION.md) for details.

### v0.5.0 (2025-11-06) - TypeScript Implementation

| Benchmark | Metric            | Result |
| --------- | ----------------- | ------ |
| DMR       | Recall per sample | 1.0    |
| LongMem   | asOf / between    | Pass   |
