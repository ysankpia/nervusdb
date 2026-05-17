# HNSW Default Recommendation

- Generated at: 2026-03-14T14:30:00+00:00
- Nodes: 10000
- Dim: 64
- Queries: 100
- K: 10

## Recommended Defaults

- Suggested params: M=16, efConstruction=200, efSearch=128
- Selection rule: lowest `vector_search_p99_ms` among runs with `recall_at_k >= 0.95`
- Metrics: recall@10=0.9520, p95=9556.29us, p99=12606.21us, vector_search_p99_ms=12.6062

## Evaluated Candidates

| M | efConstruction | efSearch | recall@k | avg us | p95 us | p99 us | vector_search_p99_ms |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 12 | 200 | 128 | 0.9000 | 6041.85 | 7771.29 | 13341.21 | 13.3412 |
| 16 | 100 | 128 | 0.9110 | 7147.88 | 7936.08 | 14749.25 | 14.7493 |
| 16 | 200 | 128 | 0.9520 | 7606.17 | 9556.29 | 12606.21 | 12.6062 |
| 16 | 200 | 200 | 0.9750 | 10411.19 | 11802.21 | 16803.00 | 16.8030 |

## Notes

- The previous recommendation (`M=8, efConstruction=100, efSearch=64`) was derived from a much smaller `nodes=200, dim=8` sweep and does not generalize to the BETA-05 gate dataset.
- On the current gate dataset, `M=16, efConstruction=200, efSearch=128` clears the recall gate with substantial latency headroom.
- Because the gate uses a single-run default configuration, these values should be treated as the new single source of truth for Nightly and local perf validation.
