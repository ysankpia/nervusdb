/**
 * 空间几何计算实现
 *
 * 提供基础的几何对象操作和空间计算功能
 */

import type {
  Geometry,
  Point,
  LineString,
  Polygon,
  MultiPoint,
  Coordinate,
  CoordinateArray,
  BoundingBox,
  SpatialGeometry,
  DistanceUnit,
} from './types.js';

/**
 * 地球半径常量（米）
 */
const EARTH_RADIUS = {
  MEAN: 6371000, // 平均半径
  EQUATORIAL: 6378137, // 赤道半径
  POLAR: 6356752.314245, // 极半径
};

/**
 * 单位转换常量
 */
const UNIT_CONVERSIONS: Record<DistanceUnit, number> = {
  meters: 1,
  kilometers: 1000,
  feet: 0.3048,
  miles: 1609.344,
  nautical_miles: 1852,
  degrees: 111320, // 大约1度经纬度对应的米数（赤道附近）
};

/**
 * 基础几何工具类
 */
export class GeometryUtils {
  /**
   * 验证坐标是否有效
   */
  static isValidCoordinate(coord: CoordinateArray): boolean {
    if (!Array.isArray(coord) || coord.length < 2 || coord.length > 3) {
      return false;
    }

    const [lon, lat, alt] = coord;

    // 检查经度范围 [-180, 180]
    if (typeof lon !== 'number' || lon < -180 || lon > 180) {
      return false;
    }

    // 检查纬度范围 [-90, 90]
    if (typeof lat !== 'number' || lat < -90 || lat > 90) {
      return false;
    }

    // 检查高程（可选）
    if (alt !== undefined && typeof alt !== 'number') {
      return false;
    }

    return true;
  }

  /**
   * 归一化坐标
   */
  static normalizeCoordinate(coord: CoordinateArray): CoordinateArray {
    const [lon, lat, alt] = coord;

    // 归一化经度到 [-180, 180]
    let normalizedLon = ((lon + 180) % 360) - 180;
    if (normalizedLon === -180) normalizedLon = 180;

    // 纬度限制在 [-90, 90]
    const normalizedLat = Math.max(-90, Math.min(90, lat));

    return alt !== undefined ? [normalizedLon, normalizedLat, alt] : [normalizedLon, normalizedLat];
  }

  /**
   * 弧度转角度
   */
  static toDegrees(radians: number): number {
    return radians * (180 / Math.PI);
  }

  /**
   * 角度转弧度
   */
  static toRadians(degrees: number): number {
    return degrees * (Math.PI / 180);
  }

  /**
   * 计算两点间的方位角
   */
  static bearing(from: Coordinate, to: Coordinate): number {
    const [lon1, lat1] = from;
    const [lon2, lat2] = to;

    const dLon = this.toRadians(lon2 - lon1);
    const lat1Rad = this.toRadians(lat1);
    const lat2Rad = this.toRadians(lat2);

    const y = Math.sin(dLon) * Math.cos(lat2Rad);
    const x =
      Math.cos(lat1Rad) * Math.sin(lat2Rad) -
      Math.sin(lat1Rad) * Math.cos(lat2Rad) * Math.cos(dLon);

    const bearing = Math.atan2(y, x);
    return (this.toDegrees(bearing) + 360) % 360;
  }

  /**
   * 根据起点、方位角和距离计算终点
   */
  static destination(start: Coordinate, bearing: number, distance: number): Coordinate {
    const [lon1, lat1] = start;
    const bearingRad = this.toRadians(bearing);
    const distanceRad = distance / EARTH_RADIUS.MEAN;

    const lat1Rad = this.toRadians(lat1);
    const lon1Rad = this.toRadians(lon1);

    const lat2Rad = Math.asin(
      Math.sin(lat1Rad) * Math.cos(distanceRad) +
        Math.cos(lat1Rad) * Math.sin(distanceRad) * Math.cos(bearingRad),
    );

    const lon2Rad =
      lon1Rad +
      Math.atan2(
        Math.sin(bearingRad) * Math.sin(distanceRad) * Math.cos(lat1Rad),
        Math.cos(distanceRad) - Math.sin(lat1Rad) * Math.sin(lat2Rad),
      );

    return [this.toDegrees(lon2Rad), this.toDegrees(lat2Rad)];
  }
}

/**
 * 空间几何计算实现
 */
export class SpatialGeometryImpl implements SpatialGeometry {
  /**
   * 计算两个几何对象间的距离
   */
  distance(geom1: Geometry, geom2: Geometry, unit: DistanceUnit = 'meters'): number {
    const dist = this.calculateDistance(geom1, geom2);
    return this.convertDistance(dist, 'meters', unit);
  }

  private calculateDistance(geom1: Geometry, geom2: Geometry): number {
    // 提取代表点进行距离计算
    const point1 = this.getRepresentativePoint(geom1);
    const point2 = this.getRepresentativePoint(geom2);

    return this.haversineDistance(point1.coordinates, point2.coordinates);
  }

  private getRepresentativePoint(geometry: Geometry): Point {
    switch (geometry.type) {
      case 'Point':
        return geometry;

      case 'LineString': {
        const lineString = geometry;
        const midIndex = Math.floor(lineString.coordinates.length / 2);
        return { type: 'Point', coordinates: lineString.coordinates[midIndex] };
      }

      case 'Polygon':
        return this.centroid(geometry);

      case 'MultiPoint': {
        const multiPoint = geometry;
        return { type: 'Point', coordinates: multiPoint.coordinates[0] };
      }

      case 'MultiLineString': {
        const multiLineString = geometry;
        return this.getRepresentativePoint({
          type: 'LineString',
          coordinates: multiLineString.coordinates[0],
        });
      }

      case 'MultiPolygon': {
        const multiPolygon = geometry;
        return this.getRepresentativePoint({
          type: 'Polygon',
          coordinates: multiPolygon.coordinates[0],
        });
      }

      case 'GeometryCollection': {
        const collection = geometry;
        if (collection.geometries.length > 0) {
          return this.getRepresentativePoint(collection.geometries[0] as unknown as Geometry);
        }
        break;
      }
    }

    // 默认返回原点
    return { type: 'Point', coordinates: [0, 0] };
  }

  /**
   * 使用Haversine公式计算两点间距离
   */
  private haversineDistance(coord1: CoordinateArray, coord2: CoordinateArray): number {
    const [lon1, lat1] = coord1;
    const [lon2, lat2] = coord2;

    const dLat = GeometryUtils.toRadians(lat2 - lat1);
    const dLon = GeometryUtils.toRadians(lon2 - lon1);

    const lat1Rad = GeometryUtils.toRadians(lat1);
    const lat2Rad = GeometryUtils.toRadians(lat2);

    const a =
      Math.sin(dLat / 2) * Math.sin(dLat / 2) +
      Math.cos(lat1Rad) * Math.cos(lat2Rad) * Math.sin(dLon / 2) * Math.sin(dLon / 2);

    const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
    return EARTH_RADIUS.MEAN * c;
  }

  /**
   * 计算几何对象面积
   */
  area(geometry: Geometry, unit: DistanceUnit = 'meters'): number {
    const areaM2 = this.calculateArea(geometry);
    const areaInUnit = this.convertArea(areaM2, 'meters', unit);
    return areaInUnit;
  }

  private calculateArea(geometry: Geometry): number {
    switch (geometry.type) {
      case 'Polygon':
        return this.polygonArea(geometry);
      case 'MultiPolygon': {
        const multiPolygon = geometry;
        return multiPolygon.coordinates.reduce((total, polygonCoords) => {
          return total + this.polygonArea({ type: 'Polygon', coordinates: polygonCoords });
        }, 0);
      }
      case 'GeometryCollection': {
        const collection = geometry;
        return collection.geometries.reduce(
          (total, geom) => total + this.calculateArea(geom as unknown as Geometry),
          0,
        );
      }
      default:
        return 0; // 点和线没有面积
    }
  }

  /**
   * 计算多边形面积（使用球面几何）
   */
  private polygonArea(polygon: Polygon): number {
    const coords = polygon.coordinates[0]; // 外环
    if (coords.length < 3) return 0;

    // 使用Girard定理计算球面多边形面积
    let area = 0;
    const radius = EARTH_RADIUS.MEAN;

    for (let i = 0; i < coords.length - 1; i++) {
      const [lon1, lat1] = coords[i];
      const [lon2, lat2] = coords[i + 1];

      const lat1Rad = GeometryUtils.toRadians(lat1);
      const lat2Rad = GeometryUtils.toRadians(lat2);
      const dLon = GeometryUtils.toRadians(lon2 - lon1);

      area += dLon * (2 + Math.sin(lat1Rad) + Math.sin(lat2Rad));
    }

    area = Math.abs((area * radius * radius) / 2);

    // 减去内环（洞）的面积
    for (let i = 1; i < polygon.coordinates.length; i++) {
      const holeArea = this.polygonArea({ type: 'Polygon', coordinates: [polygon.coordinates[i]] });
      area -= holeArea;
    }

    return area;
  }

  /**
   * 计算几何对象长度
   */
  length(geometry: Geometry, unit: DistanceUnit = 'meters'): number {
    const lengthM = this.calculateLength(geometry);
    return this.convertDistance(lengthM, 'meters', unit);
  }

  private calculateLength(geometry: Geometry): number {
    switch (geometry.type) {
      case 'LineString':
        return this.lineStringLength(geometry);
      case 'MultiLineString': {
        const multiLineString = geometry;
        return multiLineString.coordinates.reduce((total, lineCoords) => {
          return total + this.lineStringLength({ type: 'LineString', coordinates: lineCoords });
        }, 0);
      }
      case 'Polygon': {
        const polygon = geometry;
        // 计算边界长度
        return polygon.coordinates.reduce((total, ring) => {
          return total + this.lineStringLength({ type: 'LineString', coordinates: ring });
        }, 0);
      }
      case 'MultiPolygon': {
        const multiPolygon = geometry;
        return multiPolygon.coordinates.reduce((total, polygonCoords) => {
          return total + this.calculateLength({ type: 'Polygon', coordinates: polygonCoords });
        }, 0);
      }
      case 'GeometryCollection': {
        const collection = geometry;
        return collection.geometries.reduce(
          (total, geom) => total + this.calculateLength(geom as unknown as Geometry),
          0,
        );
      }
      default:
        return 0; // 点没有长度
    }
  }

  /**
   * 计算线串长度
   */
  private lineStringLength(lineString: LineString): number {
    let length = 0;
    const coords = lineString.coordinates;

    for (let i = 0; i < coords.length - 1; i++) {
      length += this.haversineDistance(coords[i], coords[i + 1]);
    }

    return length;
  }

  /**
   * 计算几何对象边界框
   */
  bounds(geometry: Geometry): BoundingBox {
    const allCoords = this.extractAllCoordinates(geometry);

    if (allCoords.length === 0) {
      return [0, 0, 0, 0];
    }

    let minX = Infinity,
      minY = Infinity;
    let maxX = -Infinity,
      maxY = -Infinity;

    for (const coord of allCoords) {
      const [x, y] = coord;
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }

    return [minX, minY, maxX, maxY];
  }

  /**
   * 提取几何对象中的所有坐标
   */
  private extractAllCoordinates(geometry: Geometry): CoordinateArray[] {
    const coords: CoordinateArray[] = [];

    const extractCoords = (geom: Geometry) => {
      switch (geom.type) {
        case 'Point':
          coords.push(geom.coordinates);
          break;

        case 'LineString':
          coords.push(...geom.coordinates);
          break;

        case 'Polygon':
          geom.coordinates.forEach((ring) => coords.push(...ring));
          break;

        case 'MultiPoint':
          coords.push(...geom.coordinates);
          break;

        case 'MultiLineString':
          geom.coordinates.forEach((line) => coords.push(...line));
          break;

        case 'MultiPolygon':
          geom.coordinates.forEach((polygon) => polygon.forEach((ring) => coords.push(...ring)));
          break;

        case 'GeometryCollection':
          geom.geometries.forEach((g) => extractCoords(g as unknown as Geometry));
          break;
      }
    };

    extractCoords(geometry);
    return coords;
  }

  /**
   * 计算几何对象中心点（边界框中心）
   */
  center(geometry: Geometry): Point {
    const bbox = this.bounds(geometry);
    const [minX, minY, maxX, maxY] = bbox;

    return {
      type: 'Point',
      coordinates: [(minX + maxX) / 2, (minY + maxY) / 2],
    };
  }

  /**
   * 计算几何对象质心
   */
  centroid(geometry: Geometry): Point {
    switch (geometry.type) {
      case 'Point':
        return geometry;

      case 'LineString':
        return this.lineStringCentroid(geometry);

      case 'Polygon':
        return this.polygonCentroid(geometry);

      case 'MultiPoint':
        return this.multiPointCentroid(geometry);

      default:
        // 对于复杂几何类型，返回边界框中心
        return this.center(geometry);
    }
  }

  private lineStringCentroid(lineString: LineString): Point {
    let totalLength = 0;
    let weightedX = 0;
    let weightedY = 0;

    const coords = lineString.coordinates;
    for (let i = 0; i < coords.length - 1; i++) {
      const [x1, y1] = coords[i];
      const [x2, y2] = coords[i + 1];
      const segmentLength = this.haversineDistance(coords[i], coords[i + 1]);

      totalLength += segmentLength;
      weightedX += ((x1 + x2) / 2) * segmentLength;
      weightedY += ((y1 + y2) / 2) * segmentLength;
    }

    if (totalLength === 0) {
      return { type: 'Point', coordinates: coords[0] };
    }

    return {
      type: 'Point',
      coordinates: [weightedX / totalLength, weightedY / totalLength],
    };
  }

  private polygonCentroid(polygon: Polygon): Point {
    const coords = polygon.coordinates[0]; // 使用外环
    let area = 0;
    let centroidX = 0;
    let centroidY = 0;

    // 使用Shoelace公式计算质心
    for (let i = 0; i < coords.length - 1; i++) {
      const [x0, y0] = coords[i];
      const [x1, y1] = coords[i + 1];

      const a = x0 * y1 - x1 * y0;
      area += a;
      centroidX += (x0 + x1) * a;
      centroidY += (y0 + y1) * a;
    }

    area /= 2;
    if (Math.abs(area) < 1e-10) {
      // 面积为0，返回第一个坐标
      return { type: 'Point', coordinates: coords[0] };
    }

    centroidX /= 6 * area;
    centroidY /= 6 * area;

    return { type: 'Point', coordinates: [centroidX, centroidY] };
  }

  private multiPointCentroid(multiPoint: MultiPoint): Point {
    const coords = multiPoint.coordinates;
    if (coords.length === 0) {
      return { type: 'Point', coordinates: [0, 0] };
    }

    const sumX = coords.reduce((sum, coord) => sum + coord[0], 0);
    const sumY = coords.reduce((sum, coord) => sum + coord[1], 0);

    return {
      type: 'Point',
      coordinates: [sumX / coords.length, sumY / coords.length],
    };
  }

  /**
   * 判断一个几何对象是否包含另一个
   */
  contains(container: Geometry, contained: Geometry): boolean {
    // 简化实现：检查contained的所有点是否都在container内
    const containedCoords = this.extractAllCoordinates(contained);
    return containedCoords.every((coord) => this.pointInGeometry(coord, container));
  }

  /**
   * 判断点是否在几何对象内
   */
  private pointInGeometry(point: CoordinateArray, geometry: Geometry): boolean {
    switch (geometry.type) {
      case 'Point': {
        const geomPoint = geometry;
        return point[0] === geomPoint.coordinates[0] && point[1] === geomPoint.coordinates[1];
      }

      case 'Polygon':
        return this.pointInPolygon(point, geometry);

      case 'MultiPolygon': {
        const multiPolygon = geometry;
        return multiPolygon.coordinates.some((polygonCoords) =>
          this.pointInPolygon(point, { type: 'Polygon', coordinates: polygonCoords }),
        );
      }

      default:
        return false;
    }
  }

  /**
   * 点在多边形内测试（Ray Casting算法）
   */
  private pointInPolygon(point: CoordinateArray, polygon: Polygon): boolean {
    const [x, y] = point;
    const coords = polygon.coordinates[0]; // 外环
    let inside = false;

    for (let i = 0, j = coords.length - 1; i < coords.length; j = i++) {
      const [xi, yi] = coords[i];
      const [xj, yj] = coords[j];

      if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) {
        inside = !inside;
      }
    }

    // 检查是否在洞内
    for (let h = 1; h < polygon.coordinates.length; h++) {
      if (this.pointInPolygon(point, { type: 'Polygon', coordinates: [polygon.coordinates[h]] })) {
        inside = false;
        break;
      }
    }

    return inside;
  }

  /**
   * 判断两个几何对象是否相交
   */
  intersects(geom1: Geometry, geom2: Geometry): boolean {
    // 首先检查边界框是否相交
    const bbox1 = this.bounds(geom1);
    const bbox2 = this.bounds(geom2);

    if (!this.bboxIntersects(bbox1, bbox2)) {
      return false;
    }

    // 简化的相交检测
    return this.geometryIntersects(geom1, geom2);
  }

  private bboxIntersects(bbox1: BoundingBox, bbox2: BoundingBox): boolean {
    const [minX1, minY1, maxX1, maxY1] = bbox1;
    const [minX2, minY2, maxX2, maxY2] = bbox2;

    return !(maxX1 < minX2 || maxX2 < minX1 || maxY1 < minY2 || maxY2 < minY1);
  }

  private geometryIntersects(geom1: Geometry, geom2: Geometry): boolean {
    // 简化实现：检查是否有任何点在对方内部
    const coords1 = this.extractAllCoordinates(geom1);
    const coords2 = this.extractAllCoordinates(geom2);

    // 检查geom1的点是否在geom2内
    for (const coord of coords1) {
      if (this.pointInGeometry(coord, geom2)) {
        return true;
      }
    }

    // 检查geom2的点是否在geom1内
    for (const coord of coords2) {
      if (this.pointInGeometry(coord, geom1)) {
        return true;
      }
    }

    return false;
  }

  /**
   * 计算两个几何对象的交集
   */
  intersection(geom1: Geometry, geom2: Geometry): Geometry | null {
    // 复杂的几何交集计算，这里提供简化实现
    if (!this.intersects(geom1, geom2)) {
      return null;
    }

    // 返回简化的交集（实际应用中需要更复杂的算法）
    const bbox1 = this.bounds(geom1);
    const bbox2 = this.bounds(geom2);
    const intersectionBbox = this.bboxIntersection(bbox1, bbox2);

    if (!intersectionBbox) {
      return null;
    }

    // 返回交集边界框对应的多边形
    const [minX, minY, maxX, maxY] = intersectionBbox;
    return {
      type: 'Polygon',
      coordinates: [
        [
          [minX, minY],
          [maxX, minY],
          [maxX, maxY],
          [minX, maxY],
          [minX, minY],
        ],
      ],
    };
  }

  private bboxIntersection(bbox1: BoundingBox, bbox2: BoundingBox): BoundingBox | null {
    const [minX1, minY1, maxX1, maxY1] = bbox1;
    const [minX2, minY2, maxX2, maxY2] = bbox2;

    const minX = Math.max(minX1, minX2);
    const minY = Math.max(minY1, minY2);
    const maxX = Math.min(maxX1, maxX2);
    const maxY = Math.min(maxY1, maxY2);

    if (minX >= maxX || minY >= maxY) {
      return null;
    }

    return [minX, minY, maxX, maxY];
  }

  /**
   * 计算两个几何对象的并集
   */
  union(geom1: Geometry, geom2: Geometry): Geometry {
    // 简化实现：返回包含两个几何对象的边界框
    const bbox1 = this.bounds(geom1);
    const bbox2 = this.bounds(geom2);

    const [minX1, minY1, maxX1, maxY1] = bbox1;
    const [minX2, minY2, maxX2, maxY2] = bbox2;

    const minX = Math.min(minX1, minX2);
    const minY = Math.min(minY1, minY2);
    const maxX = Math.max(maxX1, maxX2);
    const maxY = Math.max(maxY1, maxY2);

    return {
      type: 'Polygon',
      coordinates: [
        [
          [minX, minY],
          [maxX, minY],
          [maxX, maxY],
          [minX, maxY],
          [minX, minY],
        ],
      ],
    };
  }

  /**
   * 计算几何对象的缓冲区
   */
  buffer(geometry: Geometry, distance: number, unit: DistanceUnit = 'meters'): Geometry {
    const distanceM = this.convertDistance(distance, unit, 'meters');

    // 简化的缓冲区计算：基于边界框扩展
    const bbox = this.bounds(geometry);
    const [minX, minY, maxX, maxY] = bbox;

    // 将米转换为度（粗略估算）
    const bufferDegrees = distanceM / 111320;

    return {
      type: 'Polygon',
      coordinates: [
        [
          [minX - bufferDegrees, minY - bufferDegrees],
          [maxX + bufferDegrees, minY - bufferDegrees],
          [maxX + bufferDegrees, maxY + bufferDegrees],
          [minX - bufferDegrees, maxY + bufferDegrees],
          [minX - bufferDegrees, minY - bufferDegrees],
        ],
      ],
    };
  }

  /**
   * 简化几何对象
   */
  simplify(geometry: Geometry, tolerance: number): Geometry {
    // Douglas-Peucker算法的简化实现
    switch (geometry.type) {
      case 'LineString': {
        const simplified = this.simplifyLineString(geometry, tolerance);
        return simplified;
      }
      case 'Polygon': {
        const polygon = geometry;
        const simplifiedRings = polygon.coordinates.map((ring) =>
          this.simplifyRing(ring, tolerance),
        );
        return { type: 'Polygon', coordinates: simplifiedRings };
      }

      default:
        return geometry; // 其他类型暂不简化
    }
  }

  private simplifyLineString(lineString: LineString, tolerance: number): LineString {
    const simplified = this.douglasPeucker(lineString.coordinates, tolerance);
    return { type: 'LineString', coordinates: simplified };
  }

  private simplifyRing(ring: CoordinateArray[], tolerance: number): CoordinateArray[] {
    const simplified = this.douglasPeucker(ring.slice(0, -1), tolerance);
    simplified.push(simplified[0]); // 闭合环
    return simplified;
  }

  private douglasPeucker(points: CoordinateArray[], tolerance: number): CoordinateArray[] {
    if (points.length <= 2) return points;

    let maxDistance = 0;
    let maxIndex = 0;

    // 找到距离起点和终点连线最远的点
    for (let i = 1; i < points.length - 1; i++) {
      const distance = this.pointToLineDistance(points[i], points[0], points[points.length - 1]);
      if (distance > maxDistance) {
        maxDistance = distance;
        maxIndex = i;
      }
    }

    if (maxDistance > tolerance) {
      // 递归简化
      const left = this.douglasPeucker(points.slice(0, maxIndex + 1), tolerance);
      const right = this.douglasPeucker(points.slice(maxIndex), tolerance);

      // 合并结果（去掉重复的中间点）
      return left.slice(0, -1).concat(right);
    } else {
      // 所有中间点都可以忽略
      return [points[0], points[points.length - 1]];
    }
  }

  private pointToLineDistance(
    point: CoordinateArray,
    lineStart: CoordinateArray,
    lineEnd: CoordinateArray,
  ): number {
    const [px, py] = point;
    const [x1, y1] = lineStart;
    const [x2, y2] = lineEnd;

    const A = px - x1;
    const B = py - y1;
    const C = x2 - x1;
    const D = y2 - y1;

    const dot = A * C + B * D;
    const lenSq = C * C + D * D;

    if (lenSq === 0) {
      // 线段退化为点
      return this.haversineDistance(point, lineStart);
    }

    const param = dot / lenSq;

    let xx, yy;
    if (param < 0) {
      xx = x1;
      yy = y1;
    } else if (param > 1) {
      xx = x2;
      yy = y2;
    } else {
      xx = x1 + param * C;
      yy = y1 + param * D;
    }

    return this.haversineDistance(point, [xx, yy]);
  }

  /**
   * 验证几何对象是否有效
   */
  isValid(geometry: Geometry): boolean {
    try {
      switch (geometry.type) {
        case 'Point':
          return GeometryUtils.isValidCoordinate(geometry.coordinates);

        case 'LineString': {
          const lineString = geometry;
          return (
            lineString.coordinates.length >= 2 &&
            lineString.coordinates.every((coord) => GeometryUtils.isValidCoordinate(coord))
          );
        }

        case 'Polygon': {
          const polygon = geometry;
          return (
            polygon.coordinates.length > 0 &&
            polygon.coordinates.every(
              (ring) =>
                ring.length >= 4 && // 至少4个点（闭合）
                ring.every((coord) => GeometryUtils.isValidCoordinate(coord)) &&
                this.isRingClosed(ring),
            )
          );
        }

        default:
          return true; // 其他类型暂认为有效
      }
    } catch {
      return false;
    }
  }

  private isRingClosed(ring: CoordinateArray[]): boolean {
    if (ring.length < 4) return false;
    const first = ring[0];
    const last = ring[ring.length - 1];
    return first[0] === last[0] && first[1] === last[1];
  }

  /**
   * 修复无效的几何对象
   */
  makeValid(geometry: Geometry): Geometry {
    if (this.isValid(geometry)) {
      return geometry;
    }

    // 基本修复
    switch (geometry.type) {
      case 'Point': {
        const point = geometry;
        return {
          ...point,
          coordinates: GeometryUtils.normalizeCoordinate(point.coordinates),
        };
      }

      case 'Polygon': {
        const polygon = geometry;
        const fixedRings = polygon.coordinates.map((ring) => {
          if (!this.isRingClosed(ring) && ring.length >= 3) {
            return [...ring, ring[0]]; // 闭合环
          }
          return ring;
        });
        return { ...polygon, coordinates: fixedRings };
      }

      default:
        return geometry;
    }
  }

  /**
   * 距离单位转换
   */
  private convertDistance(distance: number, fromUnit: DistanceUnit, toUnit: DistanceUnit): number {
    if (fromUnit === toUnit) return distance;

    const metersValue = distance * UNIT_CONVERSIONS[fromUnit];
    return metersValue / UNIT_CONVERSIONS[toUnit];
  }

  /**
   * 面积单位转换
   */
  private convertArea(area: number, fromUnit: DistanceUnit, toUnit: DistanceUnit): number {
    if (fromUnit === toUnit) return area;

    const conversionFactor = UNIT_CONVERSIONS[fromUnit] / UNIT_CONVERSIONS[toUnit];
    return area * conversionFactor * conversionFactor;
  }
}
