import { describe, it, expect } from 'vitest';
import { MemoryLabelIndex } from '@/graph/labels';

describe('Labels · 内存标签索引', () => {
  it('新增/查询/删除/替换 标签', () => {
    const idx = new MemoryLabelIndex();
    idx.addNodeLabels(1, ['Person', 'Employee', 'Employee']); // 去重
    idx.addNodeLabels(2, ['Person']);
    expect(idx.getNodeLabels(1)).toEqual(['Employee', 'Person']);
    expect(Array.from(idx.findNodesByLabel('Person')).sort()).toEqual([1, 2]);

    // 交集 AND
    expect(Array.from(idx.findNodesByLabels(['Person', 'Employee'])).sort()).toEqual([1]);
    // 并集 OR
    expect(
      Array.from(idx.findNodesByLabels(['Employee', 'Unknown'], { mode: 'OR' })).sort(),
    ).toEqual([1]);

    // 替换与删除
    idx.setNodeLabels(1, ['Admin']);
    expect(idx.getNodeLabels(1)).toEqual(['Admin']);
    idx.removeNodeLabels(1, ['Admin']);
    expect(idx.getNodeLabels(1)).toEqual([]);

    // 统计
    const stats = idx.getStats();
    expect(stats.totalLabels).toBe(1);
    expect(stats.totalLabeledNodes).toBe(1);
  });
});
