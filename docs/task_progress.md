| ID | Task | Complexity | Priority | Status | Branch/PR | Notes |
|:---|:-----|:---------:|:-------:|:------:|:---------|:------|
| T1 | 索引精简并添加字符串缓存/读事务复用以提升 NervusDB 写读性能 | L2 | P0 | WIP | perf/transaction-context | 减少索引至3张表，添加字符串 LRU 缓存，优化查询读事务复用 |
