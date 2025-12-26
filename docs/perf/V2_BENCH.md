# NervusDB v2：Benchmarks & Perf Gate（T48）

目标：给 v2（M1/M2）一个最小、可重复的基准集，避免 traversal 性能回归悄悄发生。

## 运行

```bash
cargo run --example bench_v2 -p nervusdb-v2-storage --release -- \
  --nodes 50000 --degree 8 --iters 2000
```

参数：
- `--nodes`：节点数
- `--degree`：每个节点出边数（总边数 = nodes * degree）
- `--iters`：neighbors hot/cold 的重复次数

输出：
- 人类可读 summary
- 最后一行是单行 JSON（方便落盘/对比）

## 记录结果（推荐）

把 JSON 输出保存到 `docs/perf/v2/`：

```bash
bash scripts/v2_bench.sh --nodes 50000 --degree 8 --iters 2000
```

## Gate（手动门禁）

当前阶段不在 CI 默认跑重基准；release 前至少跑一次并对比上一份结果：
- `neighbors_hot_m2_edges_per_sec` 不应比上一次基线差超过 ~10%
- M2 相对 M1 的 `neighbors_hot` 至少应有明显提升（目标：数量级）

