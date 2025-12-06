/**
 * R-Tree空间索引实现
 *
 * 基于R-Tree数据结构的高效空间索引，支持快速的空间范围查询、最近邻搜索等操作
 */

import type {
  BoundingBox,
  Geometry,
  Point,
  SpatialIndex,
  SpatialIndexStats,
  SpatialQueryOptions,
  SpatialQueryResult,
  SpatialRelation,
  RTreeNode,
  RTreeItem,
  DistanceUnit,
} from './types.js';

/**
 * R-Tree配置选项
 */
export interface RTreeConfig {
  /** 最大子节点数 */
  maxEntries?: number;
  /** 最小子节点数 */
  minEntries?: number;
  /** 最大树深度 */
  maxDepth?: number;
  /** 分割策略 */
  splitStrategy?: 'linear' | 'quadratic' | 'rstar';
}

/**
 * 边界框工具函数
 */
class BboxUtils {
  /**
   * 计算边界框面积
   */
  static area(bbox: BoundingBox): number {
    return (bbox[2] - bbox[0]) * (bbox[3] - bbox[1]);
  }

  /**
   * 计算边界框周长
   */
  static perimeter(bbox: BoundingBox): number {
    return 2 * (bbox[2] - bbox[0] + (bbox[3] - bbox[1]));
  }

  /**
   * 判断边界框是否相交
   */
  static intersects(a: BoundingBox, b: BoundingBox): boolean {
    return !(a[2] < b[0] || b[2] < a[0] || a[3] < b[1] || b[3] < a[1]);
  }

  /**
   * 判断边界框a是否包含边界框b
   */
  static contains(a: BoundingBox, b: BoundingBox): boolean {
    return a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3];
  }

  /**
   * 扩展边界框以包含另一个边界框
   */
  static extend(a: BoundingBox, b: BoundingBox): BoundingBox {
    return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])];
  }

  /**
   * 计算两个边界框的交集
   */
  static intersection(a: BoundingBox, b: BoundingBox): BoundingBox | null {
    if (!this.intersects(a, b)) return null;

    return [Math.max(a[0], b[0]), Math.max(a[1], b[1]), Math.min(a[2], b[2]), Math.min(a[3], b[3])];
  }

  /**
   * 计算点到边界框的最小距离
   */
  static distanceToPoint(bbox: BoundingBox, point: [number, number]): number {
    const dx = Math.max(bbox[0] - point[0], 0, point[0] - bbox[2]);
    const dy = Math.max(bbox[1] - point[1], 0, point[1] - bbox[3]);
    return Math.sqrt(dx * dx + dy * dy);
  }

  /**
   * 从几何对象计算边界框
   */
  static fromGeometry(geometry: Geometry): BoundingBox {
    const coords = this.extractCoordinates(geometry);
    if (coords.length === 0) {
      return [0, 0, 0, 0];
    }

    let minX = Infinity,
      minY = Infinity;
    let maxX = -Infinity,
      maxY = -Infinity;

    for (const coord of coords) {
      minX = Math.min(minX, coord[0]);
      minY = Math.min(minY, coord[1]);
      maxX = Math.max(maxX, coord[0]);
      maxY = Math.max(maxY, coord[1]);
    }

    return [minX, minY, maxX, maxY];
  }

  /**
   * 从几何对象中提取所有坐标点
   */
  private static extractCoordinates(geometry: Geometry): number[][] {
    const coords: number[][] = [];

    switch (geometry.type) {
      case 'Point':
        coords.push(geometry.coordinates as number[]);
        break;

      case 'LineString':
      case 'MultiPoint':
        coords.push(...(geometry.coordinates as number[][]));
        break;

      case 'Polygon':
      case 'MultiLineString':
        for (const ring of geometry.coordinates as number[][][]) {
          coords.push(...ring);
        }
        break;

      case 'MultiPolygon':
        for (const polygon of geometry.coordinates as number[][][][]) {
          for (const ring of polygon) {
            coords.push(...ring);
          }
        }
        break;

      case 'GeometryCollection':
        for (const geom of geometry.geometries) {
          coords.push(...this.extractCoordinates(geom as Geometry));
        }
        break;
    }

    return coords;
  }
}

/**
 * R-Tree空间索引实现
 */
export class RTree implements SpatialIndex {
  private root: RTreeNode;
  private config: Required<RTreeConfig>;
  private itemCount = 0;

  constructor(config: RTreeConfig = {}) {
    this.config = {
      maxEntries: config.maxEntries || 16,
      minEntries: config.minEntries || Math.max(2, Math.ceil(config.maxEntries! * 0.4)) || 6,
      maxDepth: config.maxDepth || 10,
      splitStrategy: config.splitStrategy || 'rstar',
    };

    this.root = this.createNode([], 1, true);
  }

  /**
   * 创建新节点
   */
  private createNode(
    children: RTreeNode[] | RTreeItem[],
    height: number,
    leaf: boolean,
  ): RTreeNode {
    return {
      bbox: [Infinity, Infinity, -Infinity, -Infinity],
      children,
      leaf,
      height,
    };
  }

  /**
   * 插入几何对象
   */
  insert(geometry: Geometry, properties?: Record<string, unknown>): void {
    const bbox = BboxUtils.fromGeometry(geometry);
    const item: RTreeItem = {
      bbox,
      geometry,
      properties,
      id: this.generateId(),
    };

    this.insertItem(item, this.root.height);
    this.itemCount++;
  }

  /**
   * 插入项目到指定层级
   */
  private insertItem(item: RTreeItem, level: number): void {
    const insertPath: RTreeNode[] = [];

    // 寻找插入位置
    let node = this.root;
    const bbox = item.bbox;

    while (node.height > level) {
      insertPath.push(node);

      // 选择最适合的子节点
      node = this.chooseSubtree(bbox, node);
    }

    insertPath.push(node);

    // 插入项目
    (node.children as RTreeItem[]).push(item);
    this.extend(node.bbox, bbox);

    // 检查是否需要分裂（自下而上沿插入路径回溯）
    let idx = insertPath.length - 1;
    while (idx >= 0) {
      const cur = insertPath[idx];
      if (cur.children.length > this.config.maxEntries) {
        this.split(insertPath, idx);
        idx--;
      } else {
        break;
      }
    }

    // 调整边界框
    this.adjustParentBounds(insertPath[insertPath.length - 1].bbox, insertPath);
  }

  /**
   * 选择最适合的子树插入
   */
  private chooseSubtree(bbox: BoundingBox, node: RTreeNode): RTreeNode {
    let targetNode: RTreeNode;
    let minCost = Infinity;
    let minEnlargement = Infinity;

    for (let i = 0; i < node.children.length; i++) {
      const child = node.children[i] as RTreeNode;
      const cost = BboxUtils.area(child.bbox);
      const enlargement = BboxUtils.area(BboxUtils.extend(child.bbox, bbox)) - cost;

      if (enlargement < minEnlargement) {
        minEnlargement = enlargement;
        minCost = cost;
        targetNode = child;
      } else if (enlargement === minEnlargement) {
        if (cost < minCost) {
          minCost = cost;
          targetNode = child;
        }
      }
    }

    return targetNode!;
  }

  /**
   * 分裂节点
   */
  private split(insertPath: RTreeNode[], level: number): void {
    const node = insertPath[level];
    const M = node.children.length;
    const m = this.config.minEntries;

    this.chooseSplitAxis(node, m, M);
    const splitIndex = this.chooseSplitIndex(node, m, M);

    const spliced = node.children.splice(splitIndex, node.children.length - splitIndex);
    const newNode = this.createNode(spliced, node.height, node.leaf);

    this.calcBBox(node);
    this.calcBBox(newNode);

    if (level) (insertPath[level - 1].children as RTreeNode[]).push(newNode);
    else this.splitRoot(node, newNode);
  }

  /**
   * 分裂根节点
   */
  private splitRoot(node: RTreeNode, newNode: RTreeNode): void {
    this.root = this.createNode([node, newNode], node.height + 1, false);
    this.calcBBox(this.root);
  }

  /**
   * 选择分裂轴
   */
  private chooseSplitAxis(node: RTreeNode, m: number, M: number): void {
    const xMargin = this.allDistMargin(node, m, M, 0);
    const yMargin = this.allDistMargin(node, m, M, 1);

    if (xMargin < yMargin) {
      node.children.sort((a, b) => a.bbox[0] - b.bbox[0]);
    } else {
      node.children.sort((a, b) => a.bbox[1] - b.bbox[1]);
    }
  }

  /**
   * 计算所有分布的边界
   */
  private allDistMargin(node: RTreeNode, m: number, M: number, axis: number): number {
    node.children.sort((a, b) => a.bbox[axis] - b.bbox[axis]);

    let margin = 0;
    for (let i = m; i <= M - m; i++) {
      const bbox1 = this.distBBox(node, 0, i);
      const bbox2 = this.distBBox(node, i, M);
      margin += BboxUtils.perimeter(bbox1) + BboxUtils.perimeter(bbox2);
    }

    return margin;
  }

  /**
   * 计算分布边界框
   */
  private distBBox(node: RTreeNode, k: number, p: number): BoundingBox {
    const bbox: BoundingBox = [Infinity, Infinity, -Infinity, -Infinity];

    for (let i = k; i < p; i++) {
      this.extend(bbox, node.children[i].bbox);
    }

    return bbox;
  }

  /**
   * 选择分裂索引
   */
  private chooseSplitIndex(node: RTreeNode, m: number, M: number): number {
    let index = 0;
    let minOverlap = Infinity;
    let minArea = Infinity;

    for (let i = m; i <= M - m; i++) {
      const bbox1 = this.distBBox(node, 0, i);
      const bbox2 = this.distBBox(node, i, M);

      const overlap = this.intersectionArea(bbox1, bbox2);
      const area = BboxUtils.area(bbox1) + BboxUtils.area(bbox2);

      if (overlap < minOverlap) {
        minOverlap = overlap;
        index = i;
        minArea = area;
      } else if (overlap === minOverlap) {
        if (area < minArea) {
          minArea = area;
          index = i;
        }
      }
    }

    return index;
  }

  /**
   * 计算交集面积
   */
  private intersectionArea(bbox1: BoundingBox, bbox2: BoundingBox): number {
    const minX = Math.max(bbox1[0], bbox2[0]);
    const minY = Math.max(bbox1[1], bbox2[1]);
    const maxX = Math.min(bbox1[2], bbox2[2]);
    const maxY = Math.min(bbox1[3], bbox2[3]);

    return Math.max(0, maxX - minX) * Math.max(0, maxY - minY);
  }

  /**
   * 扩展边界框
   */
  private extend(bbox: BoundingBox, childBbox: BoundingBox): void {
    bbox[0] = Math.min(bbox[0], childBbox[0]);
    bbox[1] = Math.min(bbox[1], childBbox[1]);
    bbox[2] = Math.max(bbox[2], childBbox[2]);
    bbox[3] = Math.max(bbox[3], childBbox[3]);
  }

  /**
   * 计算节点边界框
   */
  private calcBBox(node: RTreeNode): void {
    // 重新计算并回写当前节点的 bbox（修复先前未赋值导致的 Infinity/-Infinity 残留）
    const bbox = this.distBBox(node, 0, node.children.length);
    node.bbox = [bbox[0], bbox[1], bbox[2], bbox[3]];
  }

  /**
   * 调整父节点边界框
   */
  private adjustParentBounds(bbox: BoundingBox, path: RTreeNode[]): void {
    for (let i = path.length - 1; i >= 0; i--) {
      this.extend(path[i].bbox, bbox);
    }
  }

  /**
   * 生成唯一ID
   */
  private generateId(): string {
    return Math.random().toString(36).substr(2, 9);
  }

  /**
   * 删除几何对象
   */
  remove(geometry: Geometry): boolean {
    const bbox = BboxUtils.fromGeometry(geometry);
    const path: RTreeNode[] = [];
    let node = this.root;
    let parent: RTreeNode | null = null;
    let index = -1;

    // 寻找要删除的项目
    const goingUp = true;
    while (node || path.length) {
      if (!node) {
        node = path.pop()!;
        parent = path[path.length - 1];
        index = parent ? (parent.children as RTreeNode[]).indexOf(node) : -1;
        continue;
      }

      if (node.leaf) {
        // 在叶子节点中查找项目
        const itemIndex = (node.children as RTreeItem[]).findIndex((item) =>
          this.geometryEquals(item.geometry, geometry),
        );

        if (itemIndex >= 0) {
          // 找到项目，删除它
          node.children.splice(itemIndex, 1);
          path.push(node);
          this.itemCount--;
          break;
        }
      }

      if (!goingUp && !node.leaf && BboxUtils.contains(node.bbox, bbox)) {
        // 下降到子节点
        path.push(node);
        index = 0;
        parent = node;
        node = node.children[0] as RTreeNode;
      } else if (parent) {
        // 移动到下一个兄弟节点
        index++;
        node = parent.children[index] as RTreeNode;
        if (!node) {
          parent = null;
          node = path.pop()!;
        }
      } else {
        node = undefined!;
      }
    }

    if (path.length === 0) return false;

    // 重新计算边界框并压缩树
    for (let i = path.length - 1; i >= 0; i--) {
      if (path[i].children.length === 0) {
        if (i > 0) {
          const siblings = path[i - 1].children as RTreeNode[];
          siblings.splice(siblings.indexOf(path[i]), 1);
        } else {
          this.clear();
        }
      } else {
        this.calcBBox(path[i]);
      }
    }

    return true;
  }

  /**
   * 比较两个几何对象是否相等
   */
  private geometryEquals(geom1: Geometry, geom2: Geometry): boolean {
    // 简化的几何对象比较，实际实现可能需要更复杂的逻辑
    return JSON.stringify(geom1) === JSON.stringify(geom2);
  }

  /**
   * 边界框查询
   */
  queryBoundingBox(bbox: BoundingBox, options: SpatialQueryOptions = {}): SpatialQueryResult[] {
    const results: SpatialQueryResult[] = [];
    const stack: RTreeNode[] = [this.root];

    while (stack.length) {
      const node = stack.pop()!;

      if (!BboxUtils.intersects(node.bbox, bbox)) continue;

      if (node.leaf) {
        // 叶子节点，检查每个项目
        for (const item of node.children as RTreeItem[]) {
          if (BboxUtils.intersects(item.bbox, bbox)) {
            results.push({
              geometry: item.geometry,
              properties: item.properties,
              relation: 'intersects',
            });

            if (options.limit && results.length >= options.limit) {
              return results;
            }
          }
        }
      } else {
        // 内部节点，添加子节点到栈
        stack.push(...(node.children as RTreeNode[]));
      }
    }

    return results;
  }

  /**
   * 几何对象查询
   */
  queryGeometry(
    geometry: Geometry,
    relation: SpatialRelation,
    options: SpatialQueryOptions = {},
  ): SpatialQueryResult[] {
    const bbox = BboxUtils.fromGeometry(geometry);
    const candidates = this.queryBoundingBox(bbox, options);

    // 基于几何关系进一步筛选
    return candidates.filter((result) => {
      // 这里需要实现具体的几何关系判断
      // 简化实现，实际需要更精确的几何计算
      return this.checkSpatialRelation(result.geometry, geometry, relation);
    });
  }

  /**
   * 检查空间关系
   */
  private checkSpatialRelation(
    geom1: Geometry,
    geom2: Geometry,
    relation: SpatialRelation,
  ): boolean {
    // 简化的空间关系检查，实际实现需要更复杂的几何计算
    const bbox1 = BboxUtils.fromGeometry(geom1);
    const bbox2 = BboxUtils.fromGeometry(geom2);

    switch (relation) {
      case 'intersects':
        return BboxUtils.intersects(bbox1, bbox2);
      case 'contains':
        return BboxUtils.contains(bbox1, bbox2);
      case 'within':
        return BboxUtils.contains(bbox2, bbox1);
      case 'disjoint':
        return !BboxUtils.intersects(bbox1, bbox2);
      default:
        return false;
    }
  }

  /**
   * 最近邻查询
   */
  queryNearest(
    point: Point,
    count: number,
    options: SpatialQueryOptions = {},
  ): SpatialQueryResult[] {
    const queue: Array<{ node: RTreeNode | RTreeItem; distance: number }> = [];
    const results: SpatialQueryResult[] = [];
    const pointCoords = point.coordinates as [number, number];

    queue.push({
      node: this.root,
      distance: BboxUtils.distanceToPoint(this.root.bbox, pointCoords),
    });

    while (queue.length && results.length < count) {
      queue.sort((a, b) => a.distance - b.distance);
      const { node, distance } = queue.shift()!;

      if ('geometry' in node) {
        // 叶子项目
        const result: SpatialQueryResult = {
          geometry: node.geometry,
          properties: node.properties,
          distance: options.includeDistance ? distance : undefined,
        };
        results.push(result);
      } else {
        // 内部节点
        for (const child of node.children) {
          let childDistance: number;

          if ('geometry' in child) {
            // 叶子项目
            childDistance = this.calculateDistance(point, child.geometry, options.unit);
          } else {
            // 内部节点
            childDistance = BboxUtils.distanceToPoint(child.bbox, pointCoords);
          }

          queue.push({ node: child, distance: childDistance });
        }
      }
    }

    return results;
  }

  /**
   * 范围查询
   */
  queryWithinDistance(
    point: Point,
    distance: number,
    options: SpatialQueryOptions = {},
  ): SpatialQueryResult[] {
    const pointCoords = point.coordinates as [number, number];

    // 创建搜索边界框
    const searchBbox: BoundingBox = [
      pointCoords[0] - distance,
      pointCoords[1] - distance,
      pointCoords[0] + distance,
      pointCoords[1] + distance,
    ];

    const candidates = this.queryBoundingBox(searchBbox, options);

    // 过滤出真正在距离范围内的对象
    return candidates.filter((result) => {
      const actualDistance = this.calculateDistance(point, result.geometry, options.unit);
      const withinDistance = actualDistance <= distance;

      if (withinDistance && options.includeDistance) {
        result.distance = actualDistance;
      }

      return withinDistance;
    });
  }

  /**
   * 计算两个几何对象之间的距离
   */
  private calculateDistance(
    geom1: Geometry,
    geom2: Geometry,
    unit: DistanceUnit = 'meters',
  ): number {
    // 简化的距离计算，实际实现需要更精确的几何计算
    const bbox1 = BboxUtils.fromGeometry(geom1);
    const bbox2 = BboxUtils.fromGeometry(geom2);

    const center1 = [(bbox1[0] + bbox1[2]) / 2, (bbox1[1] + bbox1[3]) / 2];
    const center2 = [(bbox2[0] + bbox2[2]) / 2, (bbox2[1] + bbox2[3]) / 2];

    return this.haversineDistance(center1 as [number, number], center2 as [number, number], unit);
  }

  /**
   * Haversine距离计算
   */
  private haversineDistance(
    coord1: [number, number],
    coord2: [number, number],
    unit: DistanceUnit,
  ): number {
    const R = unit === 'kilometers' ? 6371 : 6371000; // 地球半径
    const dLat = this.toRadians(coord2[1] - coord1[1]);
    const dLon = this.toRadians(coord2[0] - coord1[0]);
    const lat1 = this.toRadians(coord1[1]);
    const lat2 = this.toRadians(coord2[1]);

    const a =
      Math.sin(dLat / 2) * Math.sin(dLat / 2) +
      Math.sin(dLon / 2) * Math.sin(dLon / 2) * Math.cos(lat1) * Math.cos(lat2);
    const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));

    let distance = R * c;

    // 单位转换
    switch (unit) {
      case 'feet':
        distance = distance * 3.28084;
        break;
      case 'miles':
        distance = distance / 1609.344;
        break;
      case 'nautical_miles':
        distance = distance / 1852;
        break;
    }

    return distance;
  }

  /**
   * 度数转弧度
   */
  private toRadians(degrees: number): number {
    return degrees * (Math.PI / 180);
  }

  /**
   * 获取索引统计信息
   */
  getStats(): SpatialIndexStats {
    const stats = this.calculateStats(this.root);

    return {
      count: this.itemCount,
      depth: stats.depth,
      nodeCount: stats.nodeCount,
      leafCount: stats.leafCount,
      bounds: this.root.bbox,
      memoryUsage: this.estimateMemoryUsage(),
    };
  }

  /**
   * 计算统计信息
   */
  private calculateStats(node: RTreeNode): { depth: number; nodeCount: number; leafCount: number } {
    let nodeCount = 1;
    let leafCount = 0;
    let maxDepth = 1;

    if (node.leaf) {
      leafCount = 1;
    } else {
      for (const child of node.children as RTreeNode[]) {
        const childStats = this.calculateStats(child);
        nodeCount += childStats.nodeCount;
        leafCount += childStats.leafCount;
        maxDepth = Math.max(maxDepth, childStats.depth + 1);
      }
    }

    return { depth: maxDepth, nodeCount, leafCount };
  }

  /**
   * 估算内存使用量
   */
  private estimateMemoryUsage(): number {
    // 简化的内存估算
    const stats = this.calculateStats(this.root);
    const avgNodeSize = 256; // 估算每个节点的平均大小（字节）
    const avgItemSize = 512; // 估算每个项目的平均大小（字节）

    return stats.nodeCount * avgNodeSize + this.itemCount * avgItemSize;
  }

  /**
   * 清空索引
   */
  clear(): void {
    this.root = this.createNode([], 1, true);
    this.itemCount = 0;
  }

  /**
   * 序列化索引（用于持久化）
   */
  serialize(): string {
    return JSON.stringify({
      root: this.root,
      config: this.config,
      itemCount: this.itemCount,
    });
  }

  /**
   * 反序列化索引（用于加载）
   */
  static deserialize(data: string): RTree {
    const parsed = JSON.parse(data) as {
      root: RTreeNode;
      config: RTreeConfig;
      itemCount: number;
    };
    const rtree = new RTree(parsed.config);
    rtree.root = parsed.root;
    rtree.itemCount = parsed.itemCount;
    return rtree;
  }
}

/**
 * 创建R-Tree索引实例
 */
export function createRTree(config?: RTreeConfig): SpatialIndex {
  return new RTree(config);
}
