import { describe, it, expect } from 'vitest';
import { SpatialQueryManager, SpatialQueryUtils } from '@/extensions/spatial/spatialQuery.ts';

describe('空间查询管理器 · 查询/导出/聚合/验证', () => {
  it('bbox/nearest/distance/intersects/contains/within/complex/export/aggregate', () => {
    const qm = new SpatialQueryManager({ defaultUnit: 'meters', enableValidation: true });
    const pt = (x: number, y: number) => ({
      type: 'Point' as const,
      coordinates: [x, y] as [number, number],
    });
    const poly = SpatialQueryUtils.createRectangle([0, 0], [2, 2]);

    qm.addGeometry('p1', pt(0.5, 0.5), { kind: 'poi', value: 10 });
    qm.addGeometry('p2', pt(3, 3), { kind: 'poi', value: 20 });
    qm.addGeometry('a', poly, { kind: 'area', value: 5 });

    // bbox
    const bboxRes = qm.queryBoundingBox([0, 0, 2, 2], { includeDistance: true });
    expect(bboxRes.length).toBeGreaterThan(0);

    // nearest
    const nearest = qm.queryNearest(pt(0, 0), 1, { includeDistance: true });
    expect(nearest.length).toBe(1);

    // within distance
    const near = qm.queryWithinDistance(pt(0, 0), 2, { includeDistance: true });
    expect(Array.isArray(near)).toBe(true);

    // relations via geometry
    const inter = qm.queryIntersects(poly);
    expect(inter.length).toBeGreaterThan(0);
    const cont = qm.queryContains(poly);
    expect(cont.length).toBeGreaterThan(0);
    const within = qm.queryWithin(poly);
    expect(within.length).toBeGreaterThan(0);

    // complex
    const cx = qm.queryComplex({ bbox: [0, 0, 3, 3], options: { includeDistance: true } });
    expect(cx.length).toBeGreaterThan(0);

    // export to geojson
    const fc = qm.exportToGeoJSON(cx, { includeProperties: true, includeBbox: true });
    expect(fc.type).toBe('FeatureCollection');
    expect(fc.features.length).toBe(cx.length);

    // aggregate
    const agg = qm.queryAggregate({
      bbox: [0, 0, 3, 3],
      aggregations: [
        { type: 'count', alias: 'cnt' },
        { type: 'sum', field: 'value', alias: 'sum' },
        { type: 'avg', field: 'value', alias: 'avg' },
        { type: 'min', field: 'value', alias: 'min' },
        { type: 'max', field: 'value', alias: 'max' },
      ],
      groupBy: 'kind',
    });
    expect(Array.isArray(agg)).toBe(true);

    // utils validate
    const ok = SpatialQueryUtils.validateQueryParams({ bbox: [0, 0, 1, 1], distance: 1, count: 1 });
    expect(ok.valid).toBe(true);
  });
});
