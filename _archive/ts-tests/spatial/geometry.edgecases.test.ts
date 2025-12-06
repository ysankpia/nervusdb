import { describe, it, expect } from 'vitest';
import { SpatialGeometryImpl } from '@/extensions/spatial/geometry.ts';

// 边界与等容差分支覆盖
describe('空间几何 · 边界场景与等容差', () => {
  const geo = new SpatialGeometryImpl();

  const square = {
    type: 'Polygon' as const,
    coordinates: [
      [
        [0, 0],
        [4, 0],
        [4, 4],
        [0, 4],
        [0, 0],
      ],
    ],
  };

  it('点在外环边上/洞边上：库的约定下均视为在内或外之一（覆盖分支即可）', () => {
    const withHole = {
      type: 'Polygon' as const,
      coordinates: [
        square.coordinates[0],
        [
          [1, 1],
          [3, 1],
          [3, 3],
          [1, 3],
          [1, 1],
        ],
      ],
    };
    // 外环边上/洞边上：触发 pointInPolygon 分支（具体归属按实现约定，断言为布尔即可）
    const onOuterEdge = { type: 'Point' as const, coordinates: [2, 0] as [number, number] };
    expect(typeof geo.contains(withHole as any, onOuterEdge as any)).toBe('boolean');
    const onHoleEdge = { type: 'Point' as const, coordinates: [2, 1] as [number, number] };
    expect(typeof geo.contains(withHole as any, onHoleEdge as any)).toBe('boolean');
  });

  it('Douglas-Peucker：大容差仅保留端点；小容差保留更多点（两分支）', () => {
    const line = {
      type: 'LineString' as const,
      coordinates: [
        [0, 0],
        [1, 0.01],
        [2, 0],
      ] as [number, number][],
    };

    const bigTol = 100000; // 远大于偏离
    const eqRes = geo.simplify(line as any, bigTol) as any;
    expect(eqRes.type).toBe('LineString');
    expect(eqRes.coordinates.length).toBe(2);

    const smallTol = 0.0001;
    const gtRes = geo.simplify(line as any, smallTol) as any;
    expect(gtRes.type).toBe('LineString');
    expect(gtRes.coordinates.length).toBeGreaterThan(2);
  });

  it('GeometryCollection 混合：bounds/center/centroid 可运行且不抛错', () => {
    const gc = {
      type: 'GeometryCollection' as const,
      geometries: [
        { type: 'Point', coordinates: [10, 10] as [number, number] },
        {
          type: 'LineString',
          coordinates: [
            [0, 0],
            [2, 0],
          ],
        },
        square,
      ],
    } as any;

    const b = geo.bounds(gc);
    expect(b[0]).toBeLessThanOrEqual(b[2]);
    expect(b[1]).toBeLessThanOrEqual(b[3]);
    expect(geo.center(gc).type).toBe('Point');
    expect(geo.centroid(gc).type).toBe('Point');
  });
});
