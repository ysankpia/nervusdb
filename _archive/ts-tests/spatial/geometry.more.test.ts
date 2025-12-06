import { describe, it, expect } from 'vitest';
import { SpatialGeometryImpl } from '@/extensions/spatial/geometry.ts';

const geo = new SpatialGeometryImpl();

describe('空间几何 · 更多分支覆盖', () => {
  it('GeometryCollection 空集合的边界框与包含判定', () => {
    const gc = { type: 'GeometryCollection' as const, geometries: [] } as any;
    expect(geo.bounds(gc)).toEqual([0, 0, 0, 0]);
    // 注意：contains() 对空集合将视为 vacuously true（所有点都在内，因为没有点）
    const ls = {
      type: 'LineString' as const,
      coordinates: [
        [0, 0],
        [1, 1],
      ],
    };
    expect(geo.contains(ls as any, gc)).toBe(true);
  });

  it('多边形带洞：点在洞内不应被包含', () => {
    const outer = [
      [0, 0],
      [4, 0],
      [4, 4],
      [0, 4],
      [0, 0],
    ] as [number, number][];
    const hole = [
      [1, 1],
      [3, 1],
      [3, 3],
      [1, 3],
      [1, 1],
    ] as [number, number][];
    const poly = { type: 'Polygon' as const, coordinates: [outer, hole] };
    const ptInHole = { type: 'Point' as const, coordinates: [2, 2] as [number, number] };
    expect(geo.contains(poly, ptInHole)).toBe(false);
  });

  it('面积与质心：带洞多边形面积小于外环；零面积质心回退首点', () => {
    const outer = [
      [0, 0],
      [2, 0],
      [2, 2],
      [0, 2],
      [0, 0],
    ] as [number, number][];
    const hole = [
      [0.5, 0.5],
      [1.5, 0.5],
      [1.5, 1.5],
      [0.5, 1.5],
      [0.5, 0.5],
    ] as [number, number][];
    const poly = { type: 'Polygon' as const, coordinates: [outer, hole] };
    const outerOnly = { type: 'Polygon' as const, coordinates: [outer] };
    expect(geo.area(poly)).toBeLessThan(geo.area(outerOnly));

    const degenerate = {
      type: 'Polygon' as const,
      coordinates: [
        [
          [0, 0],
          [1, 1],
          [2, 2],
          [0, 0],
        ],
      ],
    };
    const centroid = geo.centroid(degenerate);
    expect(Array.isArray(centroid.coordinates)).toBe(true);
  });

  it('线串简化（Douglas-Peucker）：容差分支覆盖', () => {
    // 一条折线含多个点
    const line = {
      type: 'LineString' as const,
      coordinates: [
        [0, 0],
        [1, 0.01],
        [2, 0],
        [3, 0.02],
        [4, 0],
      ].map(([x, y]) => [x, y] as [number, number]),
    };
    // 大容差：应只保留端点
    const big = geo.simplify(line as any, 100000) as any; // 公里级容差，保留端点
    expect(big.coordinates.length).toBeLessThan(line.coordinates.length);
    // 小容差：应包含更多点
    const small = geo.simplify(line as any, 0.0001) as any;
    expect(small.coordinates.length).toBeGreaterThan(2);
  });

  it('中心点/长度：LineString 与 MultiPolygon/ MultiLineString/ MultiPoint 参与边界', () => {
    const ls = {
      type: 'LineString' as const,
      coordinates: [
        [0, 0],
        [1, 0],
        [1, 1],
      ],
    };
    expect(geo.length(ls)).toBeGreaterThan(0);
    expect(geo.center(ls).type).toBe('Point');

    const mls = {
      type: 'MultiLineString' as const,
      coordinates: [
        [
          [0, 0],
          [1, 0],
        ],
        [
          [2, 2],
          [3, 3],
        ],
      ],
    };
    const mpl = {
      type: 'MultiPolygon' as const,
      coordinates: [
        [
          [
            [0, 0],
            [1, 0],
            [1, 1],
            [0, 1],
            [0, 0],
          ],
        ],
      ],
    } as any;
    const mp = {
      type: 'MultiPoint' as const,
      coordinates: [
        [10, 10],
        [12, 12],
      ],
    };
    // 均可计算边界（不抛错即可）
    expect(Array.isArray(geo.bounds(mls))).toBe(true);
    expect(Array.isArray(geo.bounds(mpl))).toBe(true);
    expect(Array.isArray(geo.bounds(mp))).toBe(true);
  });
});
