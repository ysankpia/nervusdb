import { describe, it, expect } from 'vitest';
import { VariablePathBuilder } from '@/graph/paths.ts';

// 伪造最小 PersistentStore（仅提供 query/resolveRecords）
const store = {
  query: () => [],
  resolveRecords: () => [],
} as unknown as import('@/core/storage/persistentStore').PersistentStore;

describe('图路径工具 · paths.ts 重导出', () => {
  it('VariablePathBuilder 可被正确导入并实例化', () => {
    const builder = new VariablePathBuilder(store, new Set([1]), 2, { max: 3 });
    expect(builder).toBeInstanceOf(VariablePathBuilder as any);
    // 调用 shortest 分支（由于无数据，应返回 null）
    expect(builder.shortest(999)).toBeNull();
  });
});
