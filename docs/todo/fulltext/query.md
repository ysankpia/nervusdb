## TODO: 短语查询 slop 与高级查询支持

范围：`src/fulltext/query.ts`

待办项：

1. 短语查询 `slop` 支持
   - 目前 `processPhraseQuery()` 固定为精确短语匹配（slop=0），需要支持可配置 slop。
2. 通配符/模糊的更强表达
   - wildcard → 正则安全转换；fuzzy 可配置最大编辑距离策略。
3. 布尔查询增强
   - 现在 NOT 简化为“半拆分”，后续可支持更复杂的嵌套与优先级。

验收标准：

- slop≠0 的短语查询有单测覆盖；
- wildcard/fuzzy 的边界输入（转义/空字符串）有防护；
- 布尔查询组合在 parse→execute 的端到端测试通过。
