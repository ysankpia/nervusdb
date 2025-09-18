# 教程 09 · 常见问题与排错（FAQ）

## ESM 导入失败 / ERR_MODULE_NOT_FOUND

- 确保 Node 18+ 且工程为 ESM（`"type":"module"`）
- CommonJS 环境使用 `await import('synapsedb')`

## 全局命令找不到

- 使用 `npm i -g .`（会自动配置 PATH 中的 bin 目录）
- 若使用 pnpm，先执行 `pnpm setup` 配置 PNPM_HOME，并将其加入 PATH

## manifest 缺失或索引损坏

- `synapsedb check <db> --strict` 定位问题
- `synapsedb repair <db> [--fast]` 修复；必要时 `rebuildIndexes: true` 重新打开

## 并发写冲突

- 生产请开启 `enableLock`；读者不受写锁影响

## 结果与链式查询不稳定

- 使用 `withSnapshot(fn)` 固定 epoch，或减少治理操作与长查询并发

## 大结果集内存压力

- 使用 `streamFacts(criteria, batchSize)` 流式查询；前端/服务端用分页或流式输出
