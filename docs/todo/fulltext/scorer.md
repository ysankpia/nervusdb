## TODO: 评分器增强与配置化

范围：`src/fulltext/scorer.ts`

待办项：

1. 向量空间模型（VectorSpaceScorer）不支持 BM25
   - 当前返回“自身得分”作为占位；建议在该分支直接抛出“未支持”或保持返回但文档明确该行为。
2. CompositeScorer 参数类型与校验
   - 评分器项与权重更严格的类型与校验（权重 ≥0 且归一化）。
3. 字段权重与时间衰减配置化
   - 通过 `FullTextConfig` 或独立评分配置启用；默认值与现有保持一致。

验收标准：

- 明确 Vector 模型的 BM25 语义（文档与测试）；
- CompositeScorer 对非法权重抛出明确错误；
- 新配置不影响默认行为。
