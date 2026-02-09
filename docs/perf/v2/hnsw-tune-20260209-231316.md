# HNSW Tuning Report

- Generated at: 2026-02-09T15:13:40.426265+00:00
- Nodes: 200
- Dim: 8
- Queries: 20
- K: 5

## Recommended Defaults

- Suggested params: M=8, efConstruction=100, efSearch=64
- Metrics: recall@5=1.0000, p95=607.46us, p99=710.96us

## Sweep Results

| M | efConstruction | efSearch | recall@k | avg us | p95 us | p99 us | memory proxy bytes |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 100 | 64 | 1.0000 | 389.74 | 607.46 | 710.96 | 19706794 |
| 8 | 100 | 128 | 1.0000 | 620.22 | 1537.92 | 1918.25 | 19297194 |
