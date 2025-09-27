## TODO: Query 序列化与性能指标结构化

范围：`src/fulltext/engine.ts`

背景：当前 `queryToString()` 为“简化序列化”，`recordMetrics()/getPerformanceReport()` 仅做基础统计。

待办项：

1. 完整 Query AST → 字符串序列化与（可选）反序列化
   - 覆盖 `term/phrase/wildcard/fuzzy/boolean/range` 等类型。
   - 统一转义策略与大小写/语言处理。
2. 结构化性能指标
   - 增加 p95/p99、慢查询阈值可配置、索引命中/内存峰值等字段。
   - 为报告定义稳定的 `PerformanceReport` 类型（当前对外返回 `unknown` 升级为显式结构）。
3. 兼容与开关
   - 默认保持现状；通过 `FullTextConfig` 增加可选开关启用详细指标。

验收标准：

- 单元测试覆盖所有 Query 类型的序列化/反序列化；
- 报表结构化字段完整且具备向后兼容；
- 无性能退化（默认关闭高级统计时与当前等价）。

兼容性说明：对外 API 不变；新增字段仅添加不移除。
