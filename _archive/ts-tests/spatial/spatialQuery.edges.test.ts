import { describe, it, expect } from 'vitest';
import { SpatialQueryManager, SpatialQueryUtils } from '@/extensions/spatial/spatialQuery.ts';

// 导出/聚合/批量参数的边界分支
describe('空间查询 · 导出与聚合边界', () => {
  const pt = (x: number, y: number) => ({
    type: 'Point' as const,
    coordinates: [x, y] as [number, number],
  });

  it('exportToGeoJSON includeBbox 分支；聚合缺失字段 min/max 为空', () => {
    const qm = new SpatialQueryManager({ defaultUnit: 'meters', enableValidation: true });
    // 数据：A 有 val，B 无 val
    qm.addGeometry('a', SpatialQueryUtils.createRectangle([0, 0], [1, 1]), { cat: 'A', val: 10 });
    qm.addGeometry('b', SpatialQueryUtils.createRectangle([2, 2], [3, 3]), { cat: 'B' });
    qm.addGeometry('p', pt(0.2, 0.2), { cat: 'A', val: 5 });

    const base = qm.queryBoundingBox([0, 0, 3, 3], { includeDistance: true });

    // 不带 bbox
    const fc1 = qm.exportToGeoJSON(base, { includeProperties: true, includeBbox: false });
    expect(fc1.type).toBe('FeatureCollection');
    expect((fc1 as any).bbox).toBeUndefined();

    // 带 bbox
    const fc2 = qm.exportToGeoJSON(base, { includeProperties: true, includeBbox: true });
    expect(fc2.bbox && Array.isArray(fc2.bbox)).toBe(true);

    // 聚合：B 组没有 val，应返回 null 最小/最大
    const agg = qm.queryAggregate({
      bbox: [0, 0, 5, 5],
      aggregations: [
        { type: 'min', field: 'val', alias: 'mn' },
        { type: 'max', field: 'val', alias: 'mx' },
      ],
      groupBy: 'cat',
    });
    const rowB = agg.find((r) => r.cat === 'B');
    expect(rowB).toBeDefined();
    expect(rowB!.mn).toBeNull();
    expect(rowB!.mx).toBeNull();
  });

  it('queryBatch · geometry within 应用 limit', () => {
    const qm = new SpatialQueryManager({ defaultUnit: 'meters', enableValidation: true });
    qm.addGeometry('a', SpatialQueryUtils.createRectangle([0, 0], [2, 2]), { t: 'area' });
    qm.addGeometry('b', SpatialQueryUtils.createRectangle([1.5, 1.5], [3, 3]), { t: 'area' });

    const batches = qm.queryBatch([
      {
        type: 'geometry',
        params: {
          geometry: SpatialQueryUtils.createRectangle([0, 0], [3, 3]) as any,
          relation: 'within',
        },
        options: { limit: 1 },
      },
    ]);
    expect(Array.isArray(batches[0])).toBe(true);
    expect(batches[0].length).toBeLessThanOrEqual(1);
  });
});
