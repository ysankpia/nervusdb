import { describe, it, expect } from 'vitest';
import { SpatialQueryManager, SpatialQueryUtils } from '@/extensions/spatial/spatialQuery.ts';

const pt = (x: number, y: number) => ({
  type: 'Point' as const,
  coordinates: [x, y] as [number, number],
});

describe('空间查询 · 批量/聚合/统计/校验', () => {
  it('queryBatch 四类型/统计 reset/校验错误参数', () => {
    const qm = new SpatialQueryManager({ defaultUnit: 'meters', enableValidation: true });
    // 数据
    qm.addGeometry('a', SpatialQueryUtils.createRectangle([0, 0], [1, 1]), { cat: 'A', val: 10 });
    qm.addGeometry('b', SpatialQueryUtils.createRectangle([2, 2], [3, 3]), { cat: 'B' }); // 无 val 字段
    qm.addGeometry('p', pt(0.5, 0.5), { cat: 'P', val: 5 });

    const batches = qm.queryBatch([
      { type: 'bbox', params: { bbox: [0, 0, 1.5, 1.5] } },
      { type: 'distance', params: { center: pt(0, 0) as any, distance: 2 } },
      { type: 'nearest', params: { point: pt(0, 0) as any, count: 2 } },
      {
        type: 'geometry',
        params: {
          geometry: SpatialQueryUtils.createRectangle([0, 0], [2, 2]),
          relation: 'intersects',
        } as any,
      },
    ]);
    expect(batches.length).toBe(4);

    const agg = qm.queryAggregate({
      bbox: [0, 0, 5, 5],
      aggregations: [
        { type: 'count', alias: 'c' },
        { type: 'sum', field: 'val', alias: 's' },
        { type: 'avg', field: 'val', alias: 'a' },
        { type: 'min', field: 'val', alias: 'mn' },
        { type: 'max', field: 'val', alias: 'mx' },
      ],
      groupBy: 'cat',
    });
    expect(Array.isArray(agg)).toBe(true);

    const stats1 = qm.getQueryStats();
    expect(stats1.totalQueries).toBeGreaterThan(0);
    qm.resetStats();
    const stats2 = qm.getQueryStats();
    expect(stats2.totalQueries).toBe(0);

    // 校验错误参数
    const bad1 = SpatialQueryUtils.validateQueryParams({ bbox: [1, 1, 0, 0] });
    expect(bad1.valid).toBe(false);
    const bad2 = SpatialQueryUtils.validateQueryParams({ distance: 0 });
    expect(bad2.valid).toBe(false);
    const bad3 = SpatialQueryUtils.validateQueryParams({ count: 0 });
    expect(bad3.valid).toBe(false);
  });
});
