/**
 * 空间查询API实现
 *
 * 提供完整的地理空间查询接口，集成R-Tree索引和空间几何计算
 */

import type {
  Geometry,
  Point,
  BoundingBox,
  SpatialRelation,
  SpatialQueryOptions,
  SpatialQueryResult,
  DistanceUnit,
  SpatialIndex,
  Feature,
  FeatureCollection,
} from './types.js';
import { createRTree, RTreeConfig } from './rtree.js';
import { SpatialGeometryImpl } from './geometry.js';

/**
 * 空间查询配置
 */
export interface SpatialQueryConfig extends RTreeConfig {
  /** 默认距离单位 */
  defaultUnit?: DistanceUnit;
  /** 默认查询限制 */
  defaultLimit?: number;
  /** 是否启用几何验证 */
  enableValidation?: boolean;
  /** 精度设置 */
  precision?: number;
}

/**
 * 空间查询结果统计
 */
export interface SpatialQueryStats {
  /** 查询总数 */
  totalQueries: number;
  /** 平均查询时间（毫秒） */
  avgQueryTime: number;
  /** 索引命中次数 */
  indexHits: number;
  /** 几何计算次数 */
  geometryCalculations: number;
  /** 内存使用量 */
  memoryUsage: number;
}

/**
 * 空间查询管理器
 */
export class SpatialQueryManager {
  private spatialIndex: SpatialIndex;
  private spatialGeometry: SpatialGeometryImpl;
  private config: Required<SpatialQueryConfig>;
  private stats: SpatialQueryStats;

  constructor(config: SpatialQueryConfig = {}) {
    this.config = {
      maxEntries: config.maxEntries || 16,
      minEntries: config.minEntries || 6,
      maxDepth: config.maxDepth || 10,
      splitStrategy: config.splitStrategy || 'rstar',
      defaultUnit: config.defaultUnit || 'meters',
      defaultLimit: config.defaultLimit || 100,
      enableValidation: config.enableValidation ?? true,
      precision: config.precision || 6,
    };

    this.spatialIndex = createRTree({
      maxEntries: this.config.maxEntries,
      minEntries: this.config.minEntries,
      maxDepth: this.config.maxDepth,
      splitStrategy: this.config.splitStrategy,
    });

    this.spatialGeometry = new SpatialGeometryImpl();

    this.stats = {
      totalQueries: 0,
      avgQueryTime: 0,
      indexHits: 0,
      geometryCalculations: 0,
      memoryUsage: 0,
    };
  }

  /**
   * 添加空间对象到索引
   */
  addGeometry(id: string, geometry: Geometry, properties?: Record<string, unknown>): void {
    if (this.config.enableValidation && !this.spatialGeometry.isValid(geometry)) {
      geometry = this.spatialGeometry.makeValid(geometry);
    }

    this.spatialIndex.insert(geometry, { ...properties, id });
  }

  /**
   * 从索引中移除空间对象
   */
  removeGeometry(geometry: Geometry): boolean {
    return this.spatialIndex.remove(geometry);
  }

  /**
   * 边界框查询
   */
  queryBoundingBox(bbox: BoundingBox, options: SpatialQueryOptions = {}): SpatialQueryResult[] {
    const startTime = performance.now();
    this.stats.totalQueries++;

    const queryOptions = this.mergeOptions(options);
    const results = this.spatialIndex.queryBoundingBox(bbox, queryOptions);

    this.updateQueryStats(startTime);
    this.stats.indexHits++;

    return this.processResults(results, queryOptions);
  }

  /**
   * 点范围查询
   */
  queryWithinDistance(
    center: Point,
    distance: number,
    options: SpatialQueryOptions = {},
  ): SpatialQueryResult[] {
    const startTime = performance.now();
    this.stats.totalQueries++;

    const queryOptions = this.mergeOptions(options);
    const results = this.spatialIndex.queryWithinDistance(center, distance, queryOptions);

    this.updateQueryStats(startTime);
    this.stats.indexHits++;

    return this.processResults(results, queryOptions);
  }

  /**
   * 最近邻查询
   */
  queryNearest(
    point: Point,
    count: number = 1,
    options: SpatialQueryOptions = {},
  ): SpatialQueryResult[] {
    const startTime = performance.now();
    this.stats.totalQueries++;

    const queryOptions = this.mergeOptions(options);
    const results = this.spatialIndex.queryNearest(point, count, queryOptions);

    this.updateQueryStats(startTime);
    this.stats.indexHits++;

    return this.processResults(results, queryOptions);
  }

  /**
   * 几何对象查询
   */
  queryGeometry(
    geometry: Geometry,
    relation: SpatialRelation,
    options: SpatialQueryOptions = {},
  ): SpatialQueryResult[] {
    const startTime = performance.now();
    this.stats.totalQueries++;

    if (this.config.enableValidation && !this.spatialGeometry.isValid(geometry)) {
      geometry = this.spatialGeometry.makeValid(geometry);
    }

    const queryOptions = this.mergeOptions(options);
    const results = this.spatialIndex.queryGeometry(geometry, relation, queryOptions);

    this.updateQueryStats(startTime);
    this.stats.indexHits++;
    this.stats.geometryCalculations += results.length;

    return this.processResults(results, queryOptions);
  }

  /**
   * 空间相交查询
   */
  queryIntersects(geometry: Geometry, options: SpatialQueryOptions = {}): SpatialQueryResult[] {
    return this.queryGeometry(geometry, 'intersects', options);
  }

  /**
   * 空间包含查询
   */
  queryContains(geometry: Geometry, options: SpatialQueryOptions = {}): SpatialQueryResult[] {
    return this.queryGeometry(geometry, 'contains', options);
  }

  /**
   * 空间内部查询
   */
  queryWithin(geometry: Geometry, options: SpatialQueryOptions = {}): SpatialQueryResult[] {
    return this.queryGeometry(geometry, 'within', options);
  }

  /**
   * 复杂空间查询
   */
  queryComplex(params: {
    geometry?: Geometry;
    bbox?: BoundingBox;
    center?: Point;
    distance?: number;
    relations?: SpatialRelation[];
    filters?: Array<(result: SpatialQueryResult) => boolean>;
    options?: SpatialQueryOptions;
  }): SpatialQueryResult[] {
    const startTime = performance.now();
    this.stats.totalQueries++;

    let results: SpatialQueryResult[] = [];
    const options = this.mergeOptions(params.options || {});

    // 基础查询
    if (params.bbox) {
      results = this.spatialIndex.queryBoundingBox(params.bbox, options);
    } else if (params.center && params.distance) {
      results = this.spatialIndex.queryWithinDistance(params.center, params.distance, options);
    } else if (params.geometry && params.relations) {
      // 多关系查询
      const allResults = new Map<string, SpatialQueryResult>();

      for (const relation of params.relations) {
        const relationResults = this.spatialIndex.queryGeometry(params.geometry, relation, options);
        relationResults.forEach((result) => {
          const key = this.getResultKey(result);
          if (!allResults.has(key)) {
            allResults.set(key, result);
          }
        });
      }

      results = Array.from(allResults.values());
    } else {
      throw new Error('Invalid complex query parameters');
    }

    // 应用过滤器
    if (params.filters && params.filters.length > 0) {
      for (const filter of params.filters) {
        results = results.filter(filter);
      }
    }

    this.updateQueryStats(startTime);
    this.stats.indexHits++;

    return this.processResults(results, options);
  }

  /**
   * 空间聚合查询
   */
  queryAggregate(params: {
    geometry?: Geometry;
    bbox?: BoundingBox;
    center?: Point;
    distance?: number;
    aggregations: Array<{
      type: 'count' | 'sum' | 'avg' | 'min' | 'max';
      field?: string;
      alias?: string;
    }>;
    groupBy?: string;
    options?: SpatialQueryOptions;
  }): Record<string, string | number | null>[] {
    const startTime = performance.now();
    this.stats.totalQueries++;

    // 获取基础查询结果
    let results: SpatialQueryResult[];
    const options = this.mergeOptions(params.options || {});

    if (params.bbox) {
      results = this.spatialIndex.queryBoundingBox(params.bbox, options);
    } else if (params.center && params.distance) {
      results = this.spatialIndex.queryWithinDistance(params.center, params.distance, options);
    } else if (params.geometry) {
      results = this.spatialIndex.queryGeometry(params.geometry, 'intersects', options);
    } else {
      throw new Error('Invalid aggregate query parameters');
    }

    // 执行聚合
    const aggregateResults = this.performAggregation(results, params.aggregations, params.groupBy);

    this.updateQueryStats(startTime);
    this.stats.indexHits++;

    return aggregateResults;
  }

  /**
   * 批量查询
   */
  queryBatch(
    queries: Array<
      | { type: 'bbox'; params: { bbox: BoundingBox }; options?: SpatialQueryOptions }
      | {
          type: 'distance';
          params: { center: Point; distance: number };
          options?: SpatialQueryOptions;
        }
      | { type: 'nearest'; params: { point: Point; count?: number }; options?: SpatialQueryOptions }
      | {
          type: 'geometry';
          params: { geometry: Geometry; relation: SpatialRelation };
          options?: SpatialQueryOptions;
        }
    >,
  ): SpatialQueryResult[][] {
    const startTime = performance.now();
    this.stats.totalQueries += queries.length;

    const results = queries.map((query) => {
      const options = this.mergeOptions(query.options || {});

      switch (query.type) {
        case 'bbox':
          return this.spatialIndex.queryBoundingBox(query.params.bbox, options);
        case 'distance':
          return this.spatialIndex.queryWithinDistance(
            query.params.center,
            query.params.distance,
            options,
          );
        case 'nearest':
          return this.spatialIndex.queryNearest(
            query.params.point,
            query.params.count || 1,
            options,
          );
        case 'geometry':
          return this.spatialIndex.queryGeometry(
            query.params.geometry,
            query.params.relation,
            options,
          );
        default:
          return [];
      }
    });

    this.updateQueryStats(startTime);
    this.stats.indexHits += queries.length;

    const options2 = this.mergeOptions({});
    return results.map((result) => this.processResults(result, options2));
  }

  /**
   * 导出查询结果为GeoJSON
   */
  exportToGeoJSON(
    results: SpatialQueryResult[],
    options: { includeProperties?: boolean; includeBbox?: boolean } = {},
  ): FeatureCollection {
    const features: Feature[] = results.map((result, index) => {
      const maybeId = result.properties?.['id'];
      const idVal = typeof maybeId === 'string' || typeof maybeId === 'number' ? maybeId : index;
      return {
        type: 'Feature',
        geometry: result.geometry,
        properties: {
          ...(options.includeProperties && result.properties ? result.properties : {}),
          ...(result.distance !== undefined ? { distance: result.distance } : {}),
          ...(result.relation ? { relation: result.relation } : {}),
          _queryIndex: index,
        },
        id: idVal,
      };
    });

    const featureCollection: FeatureCollection = {
      type: 'FeatureCollection',
      features,
    };

    if (options.includeBbox && features.length > 0) {
      // 计算整体边界框
      const bounds = this.calculateBounds(results.map((r) => r.geometry));
      featureCollection.bbox = bounds;
    }

    return featureCollection;
  }

  /**
   * 获取索引统计信息
   */
  getIndexStats() {
    return this.spatialIndex.getStats();
  }

  /**
   * 获取查询统计信息
   */
  getQueryStats(): SpatialQueryStats {
    return { ...this.stats };
  }

  /**
   * 重置查询统计
   */
  resetStats(): void {
    this.stats = {
      totalQueries: 0,
      avgQueryTime: 0,
      indexHits: 0,
      geometryCalculations: 0,
      memoryUsage: 0,
    };
  }

  /**
   * 清空索引
   */
  clear(): void {
    this.spatialIndex.clear();
    this.resetStats();
  }

  /**
   * 合并查询选项
   */
  private mergeOptions(options: SpatialQueryOptions): Required<SpatialQueryOptions> {
    return {
      limit: options.limit || this.config.defaultLimit,
      unit: options.unit || this.config.defaultUnit,
      includeDistance: options.includeDistance || false,
      precision: options.precision || this.config.precision,
      crs: options.crs || 'EPSG:4326',
    };
  }

  /**
   * 处理查询结果
   */
  private processResults(
    results: SpatialQueryResult[],
    options: Required<SpatialQueryOptions>,
  ): SpatialQueryResult[] {
    return results.map((result) => ({
      ...result,
      // 精度处理
      distance:
        result.distance !== undefined
          ? this.roundToPrecision(result.distance, options.precision)
          : result.distance,
    }));
  }

  /**
   * 更新查询统计
   */
  private updateQueryStats(startTime: number): void {
    const queryTime = performance.now() - startTime;
    const totalTime = this.stats.avgQueryTime * (this.stats.totalQueries - 1) + queryTime;
    this.stats.avgQueryTime = totalTime / this.stats.totalQueries;
  }

  /**
   * 获取结果键值（用于去重）
   */
  private getResultKey(result: SpatialQueryResult): string {
    return JSON.stringify([result.geometry, result.properties?.id]);
  }

  /**
   * 执行聚合操作
   */
  private performAggregation(
    results: SpatialQueryResult[],
    aggregations: Array<{ type: string; field?: string; alias?: string }>,
    groupBy?: string,
  ): Record<string, string | number | null>[] {
    const groups = groupBy ? this.groupResults(results, groupBy) : { all: results };

    return Object.entries(groups).map(([groupKey, groupResults]) => {
      const aggregateResult: Record<string, string | number | null> = groupBy
        ? { [groupBy]: groupKey }
        : {};

      for (const agg of aggregations) {
        const alias = agg.alias || `${agg.type}_${agg.field || 'count'}`;

        switch (agg.type) {
          case 'count':
            aggregateResult[alias] = groupResults.length;
            break;
          case 'sum':
            aggregateResult[alias] = this.sumField(groupResults, agg.field!);
            break;
          case 'avg':
            aggregateResult[alias] = this.avgField(groupResults, agg.field!);
            break;
          case 'min':
            aggregateResult[alias] = this.minField(groupResults, agg.field!);
            break;
          case 'max':
            aggregateResult[alias] = this.maxField(groupResults, agg.field!);
            break;
        }
      }

      return aggregateResult;
    });
  }

  /**
   * 分组结果
   */
  private groupResults(
    results: SpatialQueryResult[],
    groupBy: string,
  ): Record<string, SpatialQueryResult[]> {
    return results.reduce(
      (groups, result) => {
        const key = result.properties?.[groupBy]?.toString() || 'null';
        if (!groups[key]) groups[key] = [];
        groups[key].push(result);
        return groups;
      },
      {} as Record<string, SpatialQueryResult[]>,
    );
  }

  /**
   * 字段求和
   */
  private sumField(results: SpatialQueryResult[], field: string): number {
    return results.reduce((sum, result) => {
      const value = result.properties?.[field];
      return sum + (typeof value === 'number' ? value : 0);
    }, 0);
  }

  /**
   * 字段平均值
   */
  private avgField(results: SpatialQueryResult[], field: string): number {
    const sum = this.sumField(results, field);
    const count = results.filter((r) => typeof r.properties?.[field] === 'number').length;
    return count > 0 ? sum / count : 0;
  }

  /**
   * 字段最小值
   */
  private minField(results: SpatialQueryResult[], field: string): number | null {
    const values = results.map((r) => r.properties?.[field]).filter((v) => typeof v === 'number');

    return values.length > 0 ? Math.min(...values) : null;
  }

  /**
   * 字段最大值
   */
  private maxField(results: SpatialQueryResult[], field: string): number | null {
    const values = results.map((r) => r.properties?.[field]).filter((v) => typeof v === 'number');

    return values.length > 0 ? Math.max(...values) : null;
  }

  /**
   * 计算几何对象集合的边界框
   */
  private calculateBounds(geometries: Geometry[]): BoundingBox {
    if (geometries.length === 0) return [0, 0, 0, 0];

    const bounds = geometries.map((geom) => this.spatialGeometry.bounds(geom));

    return [
      Math.min(...bounds.map((b) => b[0])),
      Math.min(...bounds.map((b) => b[1])),
      Math.max(...bounds.map((b) => b[2])),
      Math.max(...bounds.map((b) => b[3])),
    ];
  }

  /**
   * 数值精度处理
   */
  private roundToPrecision(value: number, precision: number): number {
    const factor = Math.pow(10, precision);
    return Math.round(value * factor) / factor;
  }
}

/**
 * 创建空间查询管理器实例
 */
export function createSpatialQuery(config?: SpatialQueryConfig): SpatialQueryManager {
  return new SpatialQueryManager(config);
}

/**
 * 空间查询工具函数
 */
export class SpatialQueryUtils {
  /**
   * 创建圆形查询区域
   */
  static createCircle(center: Point, radius: number, segments: number = 32): Geometry {
    const coordinates: import('./types.js').CoordinateArray[] = [];
    const centerCoords = center.coordinates as [number, number];

    for (let i = 0; i <= segments; i++) {
      const angle = (i * 2 * Math.PI) / segments;
      const x = centerCoords[0] + Math.cos(angle) * radius;
      const y = centerCoords[1] + Math.sin(angle) * radius;
      coordinates.push([x, y] as [number, number]);
    }

    return {
      type: 'Polygon',
      coordinates: [coordinates],
    };
  }

  /**
   * 创建矩形查询区域
   */
  static createRectangle(southwest: [number, number], northeast: [number, number]): Geometry {
    return {
      type: 'Polygon',
      coordinates: [
        [
          southwest,
          [northeast[0], southwest[1]],
          northeast,
          [southwest[0], northeast[1]],
          southwest,
        ],
      ],
    };
  }

  /**
   * 边界框转多边形
   */
  static bboxToPolygon(bbox: BoundingBox): Geometry {
    return this.createRectangle([bbox[0], bbox[1]], [bbox[2], bbox[3]]);
  }

  /**
   * 多边形转边界框
   */
  static polygonToBbox(polygon: Geometry): BoundingBox {
    const spatialGeometry = new SpatialGeometryImpl();
    return spatialGeometry.bounds(polygon);
  }

  /**
   * 验证查询参数
   */
  static validateQueryParams(params: unknown): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    let obj: Record<string, unknown> | null = null;
    if (typeof params === 'object' && params !== null) {
      obj = params as Record<string, unknown>;
    }
    if (obj && 'bbox' in obj) {
      const bbox = obj.bbox as BoundingBox;
      if (bbox.length !== 4) {
        errors.push('Bounding box must have 4 coordinates');
      } else if (bbox[0] >= bbox[2] || bbox[1] >= bbox[3]) {
        errors.push('Invalid bounding box: min values must be less than max values');
      }
    }

    if (obj && 'distance' in obj && typeof obj.distance === 'number' && obj.distance <= 0) {
      errors.push('Distance must be positive');
    }

    if (obj && 'count' in obj && typeof obj.count === 'number' && obj.count <= 0) {
      errors.push('Count must be positive');
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }
}
