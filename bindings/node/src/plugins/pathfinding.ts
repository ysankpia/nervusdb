import { NervusDBPlugin } from './base.js';
import type { NervusDB } from '../synapseDb.js';
import { PersistentStore, FactRecord } from '../core/storage/persistentStore.js';
import type { NativeDatabaseHandle } from '../native/core.js';

export interface PathfindingOptions {
  predicates?: string[];
  maxHops?: number;
  direction?: 'forward' | 'reverse' | 'both';
}

export interface WeightedPathfindingOptions {
  predicate?: string;
  maxHops?: number;
}

/**
 * 路径查询插件 (Rust Native Only)
 *
 * 所有算法均由 Rust Native 实现，不再有 TypeScript 回退。
 * 如果 Native 不可用，将抛出错误。
 *
 * 可用算法：
 * - BFS 最短路径（无权重）
 * - 双向 BFS（更高效）
 * - Dijkstra 最短路径（统一权重）
 */
export class PathfindingPlugin implements NervusDBPlugin {
  readonly name = 'pathfinding';
  readonly version = '2.0.0'; // v2.0: Rust Native Only

  private db!: NervusDB;
  private store!: PersistentStore;
  private nativeHandle!: NativeDatabaseHandle;

  initialize(db: NervusDB, store: PersistentStore): void {
    this.db = db;
    this.store = store;

    // 获取 Native handle - 必须可用
    const handle = store.getNativeHandle();
    if (!handle) {
      throw new Error(
        'PathfindingPlugin requires Rust Native addon. ' +
          'Ensure @nervusdb/core-native is installed correctly.',
      );
    }

    // 检查算法方法是否存在
    if (
      typeof handle.bfsShortestPath !== 'function' ||
      typeof handle.dijkstraShortestPath !== 'function'
    ) {
      throw new Error(
        'PathfindingPlugin requires Native algorithms (bfsShortestPath, dijkstraShortestPath). ' +
          'Your Native addon version may be outdated.',
      );
    }

    this.nativeHandle = handle;
  }

  /**
   * BFS 最短路径查询（无权重）
   *
   * @param from - 起始节点值
   * @param to - 目标节点值
   * @param options.predicates - 只考虑特定谓词的边（目前只支持第一个）
   * @param options.maxHops - 最大跳数，默认 100
   * @param options.direction - 搜索方向: 'forward' | 'reverse' | 'both'
   * @returns 路径边数组，或 null 如果无路径
   */
  shortestPath(from: string, to: string, options?: PathfindingOptions): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;

    const dir = options?.direction ?? 'forward';
    const maxHops = Math.max(1, options?.maxHops ?? 100);
    const bidirectional = dir === 'both';

    const predId = options?.predicates?.[0]
      ? this.store.getNodeIdByValue(options.predicates[0])
      : undefined;

    const result = this.nativeHandle.bfsShortestPath!(
      BigInt(startId),
      BigInt(endId),
      predId !== undefined ? BigInt(predId) : null,
      maxHops,
      bidirectional,
    );

    if (!result) return null;
    return this.pathIdsToFactRecords(result.path.map((id) => Number(id)));
  }

  /**
   * 双向 BFS 最短路径（更高效，适合大图）
   *
   * @param from - 起始节点值
   * @param to - 目标节点值
   * @param options.predicates - 只考虑特定谓词的边
   * @param options.maxHops - 最大跳数，默认 100
   * @returns 路径边数组，或 null 如果无路径
   */
  shortestPathBidirectional(
    from: string,
    to: string,
    options?: PathfindingOptions,
  ): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;
    if (startId === endId) return [];

    const maxHops = Math.max(1, options?.maxHops ?? 100);

    const predId = options?.predicates?.[0]
      ? this.store.getNodeIdByValue(options.predicates[0])
      : undefined;

    const result = this.nativeHandle.bfsShortestPath!(
      BigInt(startId),
      BigInt(endId),
      predId !== undefined ? BigInt(predId) : null,
      maxHops,
      true, // bidirectional
    );

    if (!result) return null;
    return this.pathIdsToFactRecords(result.path.map((id) => Number(id)));
  }

  /**
   * Dijkstra 最短路径（统一权重 = 1.0）
   *
   * 注意：当前 Native 实现使用统一权重，不支持自定义边权重。
   * 如需自定义权重，请使用 Cypher 查询或等待后续版本支持。
   *
   * @param from - 起始节点值
   * @param to - 目标节点值
   * @param options.predicate - 只考虑特定谓词的边
   * @param options.maxHops - 最大跳数，默认 100
   * @returns 路径边数组，或 null 如果无路径
   */
  shortestPathWeighted(
    from: string,
    to: string,
    options?: WeightedPathfindingOptions,
  ): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;

    const maxHops = Math.max(1, options?.maxHops ?? 100);

    const predicateId = options?.predicate
      ? this.store.getNodeIdByValue(options.predicate)
      : undefined;

    const result = this.nativeHandle.dijkstraShortestPath!(
      BigInt(startId),
      BigInt(endId),
      predicateId !== undefined ? BigInt(predicateId) : null,
      maxHops,
    );

    if (!result) return null;
    return this.pathIdsToFactRecords(result.path.map((id) => Number(id)));
  }

  /**
   * 将节点 ID 路径转换为 FactRecord 边路径
   */
  private pathIdsToFactRecords(nodeIds: number[]): FactRecord[] {
    if (nodeIds.length < 2) return [];

    const path: FactRecord[] = [];
    for (let i = 0; i < nodeIds.length - 1; i++) {
      const fromId = nodeIds[i];
      const toId = nodeIds[i + 1];

      // 查找连接这两个节点的边
      const edges = this.store.resolveRecords(this.store.query({ subjectId: fromId }));
      const edge = edges.find((e) => e.objectId === toId);

      if (edge) {
        path.push(edge);
      } else {
        // 尝试反向查找（用于 bidirectional 搜索）
        const edgesRev = this.store.resolveRecords(this.store.query({ objectId: fromId }));
        const edgeRev = edgesRev.find((e) => e.subjectId === toId);
        if (edgeRev) {
          path.push(edgeRev);
        }
      }
    }
    return path;
  }
}
