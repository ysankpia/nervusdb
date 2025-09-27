## TODO: 全文扩展初始化流程显式等待

范围：`src/fulltext/synapsedbExtension.ts`

背景：构造函数中调用 `this.initializeIndexes()`（内部包含 `createIndex()` 与可选的 `indexExistingData()`）。为保证构造期不阻塞，曾使用 `setTimeout` 简化等待；现已移除显式 `setTimeout`，但仍需提供“初始化完成”的可等待语义。

待办项：

1. 将初始化过程封装为 Promise，并在工厂 `SynapseDBFullTextExtensionFactory.create()` 中显式 `await`。
2. 暴露 `ready()` 方法供外部（如 CLI/测试）可选等待。

验收标准：

- 无 `setTimeout` 之类的“假等待”；
- 工厂返回前保证索引创建完成；
- 提供可等待的 `ready()`；所有测试稳定通过。

兼容性说明：对现有同步使用无破坏；仅新增可等待路径。
