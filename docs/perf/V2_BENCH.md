# NervusDB v2：Benchmarks & Perf Gate（T48）

目标：给 v2（M1/M2）一个最小、可重复的基准集，避免 traversal 性能回归悄悄发生。

## 运行

```bash
cargo run --example bench_v2 -p nervusdb-storage --release -- \
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

## 基准方法论

### 测试场景

| 场景 | 指标 | 含义 |
|-----|------|-----|
| `insert` | edges/sec | 批量创建节点和边的吞吐量 |
| `neighbors_hot` | edges/sec | 热点节点的遍历（数据常驻 CPU cache） |
| `neighbors_cold` | edges/sec | 冷节点遍历（模拟真实随机访问模式） |
| `compact` | seconds | 显式 compaction 的延迟 |

### 预期性能（参考值，macOS M2 Pro）

```json
{
  "nodes": 50000,
  "degree": 8,
  "edges": 400000,
  "insert_edges_per_sec": 200000-250000,
  "neighbors_hot_edges_per_sec": 20000000-40000000,
  "neighbors_cold_edges_per_sec": 10000000-20000000,
  "compact_secs": < 0.1
}
```

### 2025-12-30 测试结果 (macOS)

| 指标 | 值 |
|------|-----|
| insert | 227,206 edges/sec |
| neighbors_hot (M1) | 28,887,384 edges/sec |
| neighbors_hot (M2) | 21,551,231 edges/sec |
| neighbors_cold (M1) | 13,700,096 edges/sec |
| neighbors_cold (M2) | 15,880,231 edges/sec |
| compact | 0.066s |

### 对比方法

1. **同环境对比**：确保硬件、操作系统、编译器版本一致
2. **多次取中位数**：排除瞬时抖动
3. **关注趋势**：单次结果波动 5-10% 是正常的

## 示例：对比 M1 和 M2

```bash
# 运行基准
cargo run --example bench_v2 -p nervusdb-storage --release -- \
  --nodes 50000 --degree 8 --iters 2000

# 输出示例：
# neighbors_hot: M1 15000000 edges/sec, M2 30000000 edges/sec
# M2 相对 M1 提升 ~2x
```

