# Concurrency Read Profile

- Generated at: 2026-03-07T17:36:23.163571+00:00
- Nodes: 50000
- Degree: 8
- Iterations: 2000

## Current Metrics

- Hot throughput (M2): 25998631.82 edges/sec
- Cold throughput (M2): 15319551.48 edges/sec
- Hot latency: p95=0.29us, p99=0.38us
- Cold latency: p95=0.67us, p99=1.33us
- Read query P99: 0.0013ms
- Write txn latency: avg=36087.14us, p95=51849.21us, p99=61740.29us (61.7403ms)

## Baseline Comparison (concurrency-baseline.json)

- Hot throughput delta: +6644105.28 edges/sec
- Cold throughput delta: +10406512.28 edges/sec
- Hot p95 delta: -0.08us
- Cold p95 delta: -1.92us
- Read query p99 delta: +0.0000ms
- Write txn p99 delta: +0.0000ms

## Reference Comparison (historical run-*.json)

- Reference file: run-20260209-205927-compare.json
- Hot throughput delta vs reference: +12439309.79 edges/sec
- Cold throughput delta vs reference: +1605422.50 edges/sec

