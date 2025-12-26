# T48: v2 Benchmarks & Perf Gate（别让性能回归偷偷发生）

## 1. Goals

- 定义 v2 的最小基准集（M1/M2 即可跑）
- 定义 perf gate（阈值/回归判定），避免 M2/M3 改动把 traversal 性能搞崩

## 2. Bench Suite（最小）

- `bench_insert_edges`：
  - N nodes, M edges（scale 参数）
  - 指标：edges/sec、WAL bytes/op
- `bench_neighbors_hot`：
  - 固定 src，重复 neighbors（cache 热）
  - 指标：edges/sec、ns/edge
- `bench_neighbors_cold`：
  - 随机 src，模拟真实遍历（cache 冷）

## 3. Gate Strategy

- 先只在本地/手动 job 跑（CI 默认不跑重基准）
- 每个 release 前必须跑一次并记录到 `docs/perf/`（复用现有惯例）

## 4. Acceptance（示例）

- M2 之后 `neighbors_hot` 相比 M1 提升至少 10x（数量级目标）
- 同样规模下，`neighbors_hot` 不允许超过上一次基线的 +10% 回归

