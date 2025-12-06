import { describe, it, expect } from 'vitest';
import { RTree } from '@/extensions/spatial/rtree.ts';

describe('R-Tree 空间索引 · 基础查询', () => {
  it('插入/查询/删除/最近邻/范围查询', () => {
    const t = new RTree({ maxEntries: 4 });
    // 插入三个点
    const p = (x: number, y: number) => ({
      type: 'Point' as const,
      coordinates: [x, y] as [number, number],
    });
    t.insert(p(0, 0), { id: 'a' });
    t.insert(p(1, 1), { id: 'b' });
    t.insert(p(2, 2), { id: 'c' });

    // bbox 查询应返回全部
    const bboxRes = t.queryBoundingBox([-1, -1, 3, 3]);
    expect(bboxRes.length).toBe(3);

    // 最近邻以(0,0)为中心
    const nn = t.queryNearest(p(0, 0), 1, { includeDistance: true });
    expect(nn.length).toBe(1);
    expect(nn[0].distance).toBeDefined();

    // 距离范围
    const within = t.queryWithinDistance(p(0, 0), 1.5);
    expect(within.length).toBeGreaterThan(0);

    // 删除
    const ok = t.remove(p(1, 1));
    expect(ok).toBe(true);
  });
});
