import { describe, it, expect } from 'vitest';
import { GeometryUtils, SpatialGeometryImpl } from '@/extensions/spatial/geometry.ts';

describe('空间几何 · 基础与度量', () => {
  const geo = new SpatialGeometryImpl();

  it('坐标校验与归一化/弧度角度转换', () => {
    expect(GeometryUtils.isValidCoordinate([120, 30])).toBe(true);
    expect(GeometryUtils.isValidCoordinate([999, 30])).toBe(false);
    const [lon] = GeometryUtils.normalizeCoordinate([181, 30]);
    expect(lon).toBeGreaterThan(-180);
    expect(GeometryUtils.toDegrees(Math.PI)).toBeCloseTo(180, 5);
    expect(GeometryUtils.toRadians(180)).toBeCloseTo(Math.PI, 5);
  });

  it('方位角与目的地计算', () => {
    const b = GeometryUtils.bearing([0, 0], [1, 0.0001]);
    expect(b).toBeGreaterThanOrEqual(0);
    expect(b).toBeLessThan(360);
    const dest = GeometryUtils.destination([0, 0], 90, 1000);
    expect(Array.isArray(dest)).toBe(true);
  });

  it('距离/面积/长度/边界框/中心/质心', () => {
    const p1 = { type: 'Point', coordinates: [0, 0] as [number, number] } as const;
    const p2 = { type: 'Point', coordinates: [0.01, 0] as [number, number] } as const;
    const d = geo.distance(p1, p2, 'meters');
    expect(d).toBeGreaterThan(0);

    const poly = {
      type: 'Polygon',
      coordinates: [
        [
          [0, 0],
          [1, 0],
          [1, 1],
          [0, 1],
          [0, 0],
        ],
      ],
    } as const;
    expect(geo.area(poly, 'meters')).toBeGreaterThan(0);
    expect(geo.length(poly, 'meters')).toBeGreaterThan(0);
    const bbox = geo.bounds(poly);
    expect(bbox).toEqual([0, 0, 1, 1]);
    const center = geo.center(poly);
    expect(center.type).toBe('Point');
    const centroid = geo.centroid(poly);
    expect(centroid.type).toBe('Point');
  });

  it('包含/相交/交并/缓冲/简化/有效性修复', () => {
    const a = {
      type: 'Polygon',
      coordinates: [
        [
          [0, 0],
          [2, 0],
          [2, 2],
          [0, 2],
          [0, 0],
        ],
      ],
    } as any;
    const b = {
      type: 'Polygon',
      coordinates: [
        [
          [1, 1],
          [3, 1],
          [3, 3],
          [1, 3],
          [1, 1],
        ],
      ],
    } as any;
    expect(geo.intersects(a, b)).toBe(true);
    const inter = geo.intersection(a, b);
    expect(inter && inter.type).toBe('Polygon');
    const uni = geo.union(a, b);
    expect(uni.type).toBe('Polygon');
    const buf = geo.buffer({ type: 'Point', coordinates: [0, 0] }, 1, 'meters');
    expect(buf.type).toBe('Polygon');
    const simple = geo.simplify(a, 0.5);
    expect(simple.type).toBe('Polygon');
    expect(geo.isValid({ type: 'Point', coordinates: [0, 0] } as any)).toBe(true);
    const invalidPoly = {
      type: 'Polygon',
      coordinates: [
        [
          [0, 0],
          [1, 0],
          [1, 1],
        ],
      ],
    } as any; // 未闭合
    expect(geo.isValid(invalidPoly)).toBe(false);
    const fixed = geo.makeValid(invalidPoly);
    expect(geo.isValid(fixed)).toBe(true);
  });
});
