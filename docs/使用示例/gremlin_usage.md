# Gremlin 使用指南

## 基础

NervusDB 提供与 Gremlin 兼容的查询接口（通过 `src/query/gremlin/*`）。在 Node.js 中可这样调用：

```ts
import { GremlinExecutor } from '@/query/gremlin/executor';

const executor = await GremlinExecutor.open('social.nervusdb');
const result = await executor.execute("g.V('user:alice').repeat(out('FRIEND_OF')).times(2).path()");
```

## 支持的步骤

| 步骤                        | 状态         |
| --------------------------- | ------------ |
| `V()` / `E()`               | ✅           |
| `out()` / `in()` / `both()` | ✅           |
| `repeat()` + `times()`      | ✅           |
| `path()`                    | ✅           |
| `valueMap()`                | ✅           |
| `has()`                     | ✅           |
| `limit()` / `range()`       | ✅           |
| `order()`                   | ⬜（进行中） |
| `union()`                   | ⬜           |

## 示例

```javascript
g.V('user:alice').repeat(out('FRIEND_OF')).times(2).valueMap('labels', 'dept');
```

## 注意事项

- 数据类型以字符串表示，属性通过 `valueMap` 访问
- Path 结果为数组数组，需要自行格式化
- 建议在应用层包装常用查询

## 故障排查

| 现象             | 解决                                                   |
| ---------------- | ------------------------------------------------------ |
| Unsupported step | 当前版本未实现该步骤，查看 `src/query/gremlin/step.ts` |
| 结果为空         | 确认起点是否存在，或路径长度设置是否合理               |
| 性能慢           | 减少 `repeat` 深度或加入筛选条件                       |

## 延伸阅读

- [docs/使用示例/03-查询与联想-示例.md](03-查询与联想-示例.md)
- [docs/教学文档/教程-03-查询与链式联想.md](../教学文档/教程-03-查询与链式联想.md)
