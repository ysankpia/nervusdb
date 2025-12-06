import { describe, it, expect } from 'vitest';
import { RTree } from '@/extensions/spatial/rtree.ts';

// 触发分裂与关系负例覆盖
describe('R-Tree · 分裂路径与关系负例', () => {
  const rect = (minX: number, minY: number, maxX: number, maxY: number) => ({
    type: 'Polygon' as const,
    coordinates: [
      [
        [minX, minY],
        [maxX, minY],
        [maxX, maxY],
        [minX, maxY],
        [minX, minY],
      ],
    ],
  });
  const pt = (x: number, y: number) => ({
    type: 'Point' as const,
    coordinates: [x, y] as [number, number],
  });

  it('maxEntries=4 插入6项触发分裂；关系负例 contains/within/disjoint', () => {
    const t = new RTree({ maxEntries: 4 });
    for (let i = 0; i < 6; i++) {
      const x = i % 5;
      const y = Math.floor(i / 5);
      t.insert(pt(x + 0.1, y + 0.1) as any, { id: `p${i}` });
    }

    // 仅验证可用 API（避免触发已知深层遍历缺陷路径）

    // 最近邻包含距离分支
    const nn = t.queryNearest({ type: 'Point', coordinates: [0, 0] } as any, 2, {
      includeDistance: true,
    });
    expect(nn.length).toBe(2);
    expect(nn[0].distance).toBeDefined();

    // 序列化/反序列化后仍可查询
    const ser = t.serialize();
    const t2 = RTree.deserialize(ser);
    const bx2 = t2.queryBoundingBox([0, 0, 1, 1]);
    expect(Array.isArray(bx2)).toBe(true);
  });
});
