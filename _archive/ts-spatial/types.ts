/**
 * 地理空间数据类型定义
 *
 * 实现GeoJSON兼容的空间数据类型，支持多种几何形状和空间索引
 */

// GeoJSON基础类型
export type GeoJSONGeometryType =
  | 'Point'
  | 'LineString'
  | 'Polygon'
  | 'MultiPoint'
  | 'MultiLineString'
  | 'MultiPolygon'
  | 'GeometryCollection';

export type GeoJSONFeatureType = 'Feature' | 'FeatureCollection';

/**
 * 基础坐标类型
 */
export type Coordinate = [number, number]; // [longitude, latitude]
export type Coordinate3D = [number, number, number]; // [longitude, latitude, elevation]
export type CoordinateArray = Coordinate | Coordinate3D;

/**
 * 边界框类型
 */
export type BoundingBox = [number, number, number, number]; // [minX, minY, maxX, maxY]
export type BoundingBox3D = [number, number, number, number, number, number]; // [minX, minY, minZ, maxX, maxY, maxZ]

/**
 * 判断输入是否为合法的二维边界框
 */
export function isBoundingBox(value: unknown): value is BoundingBox {
  if (!Array.isArray(value) || value.length !== 4) return false;

  return value.every((item) => typeof item === 'number' && Number.isFinite(item));
}

/**
 * 断言输入必须为二维边界框
 */
export function assertBoundingBox(value: unknown, message?: string): asserts value is BoundingBox {
  if (isBoundingBox(value)) return;

  throw new TypeError(message ?? '空间边界框格式错误：期望 [minX, minY, maxX, maxY] 数组');
}

/**
 * GeoJSON几何对象接口
 */
export interface GeoJSONGeometry {
  type: GeoJSONGeometryType;
  coordinates: unknown;
  bbox?: BoundingBox | BoundingBox3D;
}

/**
 * 点几何
 */
export interface Point extends GeoJSONGeometry {
  type: 'Point';
  coordinates: CoordinateArray;
}

/**
 * 线串几何
 */
export interface LineString extends GeoJSONGeometry {
  type: 'LineString';
  coordinates: CoordinateArray[];
}

/**
 * 多边形几何
 */
export interface Polygon extends GeoJSONGeometry {
  type: 'Polygon';
  coordinates: CoordinateArray[][];
}

/**
 * 多点几何
 */
export interface MultiPoint extends GeoJSONGeometry {
  type: 'MultiPoint';
  coordinates: CoordinateArray[];
}

/**
 * 多线串几何
 */
export interface MultiLineString extends GeoJSONGeometry {
  type: 'MultiLineString';
  coordinates: CoordinateArray[][];
}

/**
 * 多多边形几何
 */
export interface MultiPolygon extends GeoJSONGeometry {
  type: 'MultiPolygon';
  coordinates: CoordinateArray[][][];
}

/**
 * 几何集合
 */
export interface GeometryCollection extends Omit<GeoJSONGeometry, 'coordinates'> {
  type: 'GeometryCollection';
  geometries: GeoJSONGeometry[];
}

/**
 * 联合几何类型
 */
export type Geometry =
  | Point
  | LineString
  | Polygon
  | MultiPoint
  | MultiLineString
  | MultiPolygon
  | GeometryCollection;

/**
 * GeoJSON要素对象
 */
export interface Feature {
  type: 'Feature';
  geometry: Geometry | null;
  properties: Record<string, unknown> | null;
  id?: string | number;
  bbox?: BoundingBox | BoundingBox3D;
}

/**
 * GeoJSON要素集合
 */
export interface FeatureCollection {
  type: 'FeatureCollection';
  features: Feature[];
  bbox?: BoundingBox | BoundingBox3D;
}

/**
 * GeoJSON对象类型联合
 */
export type GeoJSON = Geometry | Feature | FeatureCollection;

/**
 * 空间关系类型
 */
export type SpatialRelation =
  | 'intersects' // 相交
  | 'contains' // 包含
  | 'within' // 在内部
  | 'touches' // 相切
  | 'crosses' // 穿过
  | 'overlaps' // 重叠
  | 'disjoint' // 分离
  | 'equals'; // 相等

/**
 * 距离单位
 */
export type DistanceUnit =
  | 'meters'
  | 'kilometers'
  | 'feet'
  | 'miles'
  | 'nautical_miles'
  | 'degrees';

/**
 * 空间查询选项
 */
export interface SpatialQueryOptions {
  /** 最大结果数量 */
  limit?: number;
  /** 距离单位 */
  unit?: DistanceUnit;
  /** 是否包含距离信息 */
  includeDistance?: boolean;
  /** 精度（小数点位数） */
  precision?: number;
  /** 坐标参考系统 */
  crs?: string;
}

/**
 * 空间查询结果
 */
export interface SpatialQueryResult {
  /** 匹配的几何对象 */
  geometry: Geometry;
  /** 关联的属性 */
  properties?: Record<string, unknown>;
  /** 距离（如果请求） */
  distance?: number;
  /** 匹配的空间关系 */
  relation?: SpatialRelation;
}

/**
 * 空间索引接口
 */
export interface SpatialIndex {
  /** 插入几何对象 */
  insert(geometry: Geometry, properties?: Record<string, unknown>): void;

  /** 删除几何对象 */
  remove(geometry: Geometry): boolean;

  /** 边界框查询 */
  queryBoundingBox(bbox: BoundingBox, options?: SpatialQueryOptions): SpatialQueryResult[];

  /** 几何对象查询 */
  queryGeometry(
    geometry: Geometry,
    relation: SpatialRelation,
    options?: SpatialQueryOptions,
  ): SpatialQueryResult[];

  /** 最近邻查询 */
  queryNearest(point: Point, count: number, options?: SpatialQueryOptions): SpatialQueryResult[];

  /** 范围查询 */
  queryWithinDistance(
    point: Point,
    distance: number,
    options?: SpatialQueryOptions,
  ): SpatialQueryResult[];

  /** 获取索引统计信息 */
  getStats(): SpatialIndexStats;

  /** 清空索引 */
  clear(): void;
}

/**
 * 空间索引统计信息
 */
export interface SpatialIndexStats {
  /** 索引中的对象数量 */
  count: number;
  /** 索引深度 */
  depth: number;
  /** 节点数量 */
  nodeCount: number;
  /** 叶子节点数量 */
  leafCount: number;
  /** 索引边界框 */
  bounds: BoundingBox;
  /** 内存使用量（字节） */
  memoryUsage: number;
}

/**
 * R-Tree索引节点
 */
export interface RTreeNode {
  /** 节点边界框 */
  bbox: BoundingBox;
  /** 子节点或叶子项 */
  children: RTreeNode[] | RTreeItem[];
  /** 是否为叶子节点 */
  leaf: boolean;
  /** 节点高度 */
  height: number;
}

/**
 * R-Tree叶子项
 */
export interface RTreeItem {
  /** 项目边界框 */
  bbox: BoundingBox;
  /** 关联的几何对象 */
  geometry: Geometry;
  /** 关联的属性 */
  properties?: Record<string, unknown>;
  /** 唯一标识 */
  id?: string;
}

/**
 * 空间几何计算接口
 */
export interface SpatialGeometry {
  /** 计算两点间距离 */
  distance(geom1: Geometry, geom2: Geometry, unit?: DistanceUnit): number;

  /** 计算几何对象面积 */
  area(geometry: Geometry, unit?: DistanceUnit): number;

  /** 计算几何对象长度 */
  length(geometry: Geometry, unit?: DistanceUnit): number;

  /** 计算几何对象边界框 */
  bounds(geometry: Geometry): BoundingBox;

  /** 计算几何对象中心点 */
  center(geometry: Geometry): Point;

  /** 计算几何对象质心 */
  centroid(geometry: Geometry): Point;

  /** 判断点是否在几何对象内 */
  contains(container: Geometry, contained: Geometry): boolean;

  /** 判断两个几何对象是否相交 */
  intersects(geom1: Geometry, geom2: Geometry): boolean;

  /** 计算两个几何对象的交集 */
  intersection(geom1: Geometry, geom2: Geometry): Geometry | null;

  /** 计算两个几何对象的并集 */
  union(geom1: Geometry, geom2: Geometry): Geometry;

  /** 计算几何对象的缓冲区 */
  buffer(geometry: Geometry, distance: number, unit?: DistanceUnit): Geometry;

  /** 简化几何对象 */
  simplify(geometry: Geometry, tolerance: number): Geometry;

  /** 验证几何对象是否有效 */
  isValid(geometry: Geometry): boolean;

  /** 修复无效的几何对象 */
  makeValid(geometry: Geometry): Geometry;
}

/**
 * 坐标参考系统接口
 */
export interface CoordinateReferenceSystem {
  /** CRS名称 */
  name: string;
  /** EPSG代码 */
  epsg?: number;
  /** WKT定义 */
  wkt?: string;
  /** 坐标转换函数 */
  transform?(from: CoordinateArray, toCrs: CoordinateReferenceSystem): CoordinateArray;
}

/**
 * 空间数据存储接口
 */
export interface SpatialStore {
  /** 存储空间对象 */
  store(id: string, geometry: Geometry, properties?: Record<string, unknown>): void;

  /** 获取空间对象 */
  get(id: string): { geometry: Geometry; properties?: Record<string, unknown> } | null;

  /** 删除空间对象 */
  delete(id: string): boolean;

  /** 更新空间对象 */
  update(id: string, geometry: Geometry, properties?: Record<string, unknown>): boolean;

  /** 查询所有对象ID */
  keys(): string[];

  /** 获取存储统计 */
  stats(): { count: number; memoryUsage: number };
}

/**
 * 空间操作配置
 */
export interface SpatialConfig {
  /** 默认坐标参考系统 */
  defaultCrs?: CoordinateReferenceSystem;
  /** 默认精度 */
  defaultPrecision?: number;
  /** 默认距离单位 */
  defaultUnit?: DistanceUnit;
  /** 索引配置 */
  indexConfig?: {
    maxEntries?: number;
    minEntries?: number;
    maxDepth?: number;
  };
}

/**
 * 地理编码结果
 */
export interface GeocodingResult {
  /** 地址 */
  address: string;
  /** 坐标 */
  coordinates: Coordinate;
  /** 置信度 */
  confidence: number;
  /** 边界框 */
  bbox?: BoundingBox;
  /** 附加属性 */
  properties?: Record<string, unknown>;
}

/**
 * 地理编码服务接口
 */
export interface GeocodingService {
  /** 正向地理编码（地址转坐标） */
  geocode(address: string): Promise<GeocodingResult[]>;

  /** 反向地理编码（坐标转地址） */
  reverseGeocode(coordinate: Coordinate): Promise<GeocodingResult[]>;
}

/**
 * 路径规划结果
 */
export interface RoutingResult {
  /** 路径几何 */
  geometry: LineString;
  /** 总距离 */
  distance: number;
  /** 总时间（秒） */
  duration: number;
  /** 路径指令 */
  instructions?: RoutingInstruction[];
}

/**
 * 路径指令
 */
export interface RoutingInstruction {
  /** 指令文本 */
  text: string;
  /** 距离 */
  distance: number;
  /** 时间 */
  duration: number;
  /** 方位角 */
  bearing?: number;
  /** 指令类型 */
  type: 'turn' | 'straight' | 'arrival' | 'departure';
}

/**
 * 路径规划服务接口
 */
export interface RoutingService {
  /** 计算路径 */
  route(start: Point, end: Point, waypoints?: Point[]): Promise<RoutingResult>;

  /** 计算等时线 */
  isochrone(center: Point, time: number): Promise<Polygon>;
}

/**
 * 空间分析工具接口
 */
export interface SpatialAnalysis {
  /** 空间聚类 */
  cluster(
    points: Point[],
    algorithm?: 'dbscan' | 'kmeans',
    options?: Record<string, unknown>,
  ): Point[][];

  /** 热力图生成 */
  heatmap(
    points: Point[],
    weights?: number[],
    options?: { radius: number; blur: number },
  ): Geometry;

  /** 空间插值 */
  interpolate(
    points: Point[],
    values: number[],
    method?: 'idw' | 'kriging',
  ): (point: Point) => number;

  /** 空间统计 */
  spatialStats(geometries: Geometry[]): {
    count: number;
    area: number;
    length: number;
    bounds: BoundingBox;
    center: Point;
  };
}
