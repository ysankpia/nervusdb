import { describe, it } from 'vitest';

// 说明：为避免 CI 过慢，此用例默认跳过。
// 如需本地验证大规模插入性能，可将 `it.skip` 改为 `it` 并调整规模。

describe('性能大规模插入（占位）', () => {
  it.skip('插入 100k 记录的端到端耗时评估（本地启用）', async () => {
    // 本地建议：
    // 1) 打开数据库（开启 rebuildIndexes）
    // 2) beginBatch 批量 addFact + 属性
    // 3) commit + flush
    // 4) 统计耗时与页规模
    // 该用例为占位，后续可根据硬件环境放开验证。
  });
});
