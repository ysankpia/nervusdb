import { SynapseDBPlugin } from './base.js';
import type { SynapseDB } from '../synapseDb.js';
import { PersistentStore, FactRecord } from '../storage/persistentStore.js';
import { MinHeap } from '../utils/minHeap.js';

export interface PathfindingOptions {
  predicates?: string[];
  maxHops?: number;
  direction?: 'forward' | 'reverse' | 'both';
}

export interface WeightedPathfindingOptions {
  predicate?: string;
  weightProperty?: string;
}

/**
 * 路径查询插件
 *
 * 提供各种路径查询算法：
 * - BFS最短路径
 * - 双向BFS
 * - Dijkstra加权最短路径
 */
export class PathfindingPlugin implements SynapseDBPlugin {
  readonly name = 'pathfinding';
  readonly version = '1.0.0';

  private db!: SynapseDB;
  private store!: PersistentStore;

  initialize(db: SynapseDB, store: PersistentStore): void {
    this.db = db;
    this.store = store;
  }

  /**
   * BFS最短路径查询
   */
  shortestPath(from: string, to: string, options?: PathfindingOptions): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;

    const dir = options?.direction ?? 'forward';
    const maxHops = Math.max(1, options?.maxHops ?? 8);
    const predIds: number[] | null = options?.predicates
      ? options.predicates
          .map((p) => this.store.getNodeIdByValue(p))
          .filter((x): x is number => typeof x === 'number')
      : null;

    const getNeighbors = (nid: number): FactRecord[] => {
      const outs: FactRecord[] = [];
      const pushMatches = (
        criteria: Partial<{ subjectId: number; predicateId: number; objectId: number }>,
      ) => {
        const enc = this.store.query(criteria);
        outs.push(...this.store.resolveRecords(enc));
      };

      if (dir === 'forward' || dir === 'both') {
        if (predIds && predIds.length > 0) {
          for (const pid of predIds) pushMatches({ subjectId: nid, predicateId: pid });
        } else {
          pushMatches({ subjectId: nid });
        }
      }
      if (dir === 'reverse' || dir === 'both') {
        if (predIds && predIds.length > 0) {
          for (const pid of predIds) pushMatches({ predicateId: pid, objectId: nid });
        } else {
          pushMatches({ objectId: nid });
        }
      }
      return outs;
    };

    const queue: Array<{ node: number; path: FactRecord[] }> = [{ node: startId, path: [] }];
    const visited = new Set<number>([startId]);
    let depth = 0;

    while (queue.length > 0 && depth <= maxHops) {
      const levelSize = queue.length;
      for (let i = 0; i < levelSize; i++) {
        const cur = queue.shift()!;
        if (cur.node === endId) return cur.path;

        const neighbors = getNeighbors(cur.node);
        for (const e of neighbors) {
          const nextNode = e.subjectId === cur.node ? e.objectId : e.subjectId;
          if (visited.has(nextNode)) continue;
          visited.add(nextNode);
          queue.push({ node: nextNode, path: [...cur.path, e] });
        }
      }
      depth += 1;
    }
    return null;
  }

  /**
   * 双向BFS最短路径（更高效）
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

    const maxHops = Math.max(1, options?.maxHops ?? 8);
    const predIds: number[] | null = options?.predicates
      ? options.predicates
          .map((p) => this.store.getNodeIdByValue(p))
          .filter((x): x is number => typeof x === 'number')
      : null;

    // 缓存查询结果
    const forwardCache = new Map<number, FactRecord[]>();
    const backwardCache = new Map<number, FactRecord[]>();

    const neighborsForward = (nid: number): FactRecord[] => {
      if (forwardCache.has(nid)) return forwardCache.get(nid)!;

      const out: FactRecord[] = [];
      const pushMatches = (
        criteria: Partial<{ subjectId: number; predicateId: number; objectId: number }>,
      ) => {
        const enc = this.store.query(criteria);
        out.push(...this.store.resolveRecords(enc));
      };

      if (predIds && predIds.length > 0) {
        for (const pid of predIds) pushMatches({ subjectId: nid, predicateId: pid });
      } else {
        pushMatches({ subjectId: nid });
      }

      forwardCache.set(nid, out);
      return out;
    };

    const neighborsBackward = (nid: number): FactRecord[] => {
      if (backwardCache.has(nid)) return backwardCache.get(nid)!;

      const out: FactRecord[] = [];
      const pushMatches = (
        criteria: Partial<{ subjectId: number; predicateId: number; objectId: number }>,
      ) => {
        const enc = this.store.query(criteria);
        out.push(...this.store.resolveRecords(enc));
      };

      if (predIds && predIds.length > 0) {
        for (const pid of predIds) pushMatches({ predicateId: pid, objectId: nid });
      } else {
        pushMatches({ objectId: nid });
      }

      backwardCache.set(nid, out);
      return out;
    };

    const prevFrom = new Map<number, FactRecord>();
    const nextTo = new Map<number, FactRecord>();

    const visitedFrom = new Set<number>([startId]);
    const visitedTo = new Set<number>([endId]);
    let frontierFrom = new Set<number>([startId]);
    let frontierTo = new Set<number>([endId]);

    let hops = 0;
    let meet: number | null = null;

    while (frontierFrom.size > 0 && frontierTo.size > 0 && hops < maxHops / 2 + 1) {
      hops += 1;

      // 选择较小的一侧扩展
      if (frontierFrom.size <= frontierTo.size) {
        const nextFrontier = new Set<number>();
        for (const u of frontierFrom) {
          const neighbors = neighborsForward(u);
          for (const e of neighbors) {
            const v = e.objectId;
            if (visitedFrom.has(v)) continue;

            visitedFrom.add(v);
            prevFrom.set(v, e);

            if (visitedTo.has(v)) {
              meet = v;
              break;
            }

            nextFrontier.add(v);
          }
          if (meet !== null) break;
        }
        if (meet !== null) break;
        frontierFrom = nextFrontier;
      } else {
        const nextFrontier = new Set<number>();
        for (const u of frontierTo) {
          const neighbors = neighborsBackward(u);
          for (const e of neighbors) {
            const v = e.subjectId;
            if (visitedTo.has(v)) continue;

            visitedTo.add(v);
            nextTo.set(v, e);

            if (visitedFrom.has(v)) {
              meet = v;
              break;
            }

            nextFrontier.add(v);
          }
          if (meet !== null) break;
        }
        if (meet !== null) break;
        frontierTo = nextFrontier;
      }
    }

    if (meet === null) return null;

    // 重建路径
    const path: FactRecord[] = [];

    // 回溯 start -> meet
    const leftPath: FactRecord[] = [];
    let cur = meet;
    while (cur !== startId && prevFrom.has(cur)) {
      const e = prevFrom.get(cur)!;
      leftPath.push(e);
      cur = e.subjectId;
    }

    // 正向遍历 start -> meet
    for (let i = leftPath.length - 1; i >= 0; i--) {
      path.push(leftPath[i]);
    }

    // 正向拼接 meet -> end
    cur = meet;
    while (cur !== endId && nextTo.has(cur)) {
      const e = nextTo.get(cur)!;
      path.push(e);
      cur = e.objectId;
    }

    return path;
  }

  /**
   * Dijkstra加权最短路径
   */
  shortestPathWeighted(
    from: string,
    to: string,
    options?: WeightedPathfindingOptions,
  ): FactRecord[] | null {
    const startId = this.store.getNodeIdByValue(from);
    const endId = this.store.getNodeIdByValue(to);
    if (startId === undefined || endId === undefined) return null;

    const predicateId = options?.predicate
      ? this.store.getNodeIdByValue(options.predicate)
      : undefined;
    const weightKey = options?.weightProperty ?? 'weight';

    const dist = new Map<number, number>();
    const prev = new Map<number, FactRecord | null>();
    const visited = new Set<number>();
    dist.set(startId, 0);

    const queue = new MinHeap<{ node: number; d: number }>((a, b) => a.d - b.d);
    queue.push({ node: startId, d: 0 });

    while (!queue.isEmpty()) {
      const { node } = queue.pop()!;
      if (visited.has(node)) continue;
      visited.add(node);
      if (node === endId) break;

      const criteria: { subjectId: number; predicateId?: number } =
        predicateId !== undefined ? { subjectId: node, predicateId } : { subjectId: node };
      const enc = this.store.query(criteria);
      const edges = this.store.resolveRecords(enc);

      for (const e of edges) {
        const rawWeight = e.edgeProperties ? e.edgeProperties[weightKey] : undefined;
        const w = Number(rawWeight ?? 1);
        const alt = (dist.get(node) ?? Infinity) + (Number.isFinite(w) ? w : 1);
        const v = e.objectId;
        if (alt < (dist.get(v) ?? Infinity)) {
          dist.set(v, alt);
          prev.set(v, e);
          queue.push({ node: v, d: alt });
        }
      }
    }

    if (!dist.has(endId)) return null;

    const path: FactRecord[] = [];
    let cur = endId;
    while (cur !== startId) {
      const edge = prev.get(cur);
      if (!edge) break;
      path.push(edge);
      cur = edge.subjectId;
    }
    path.reverse();
    return path;
  }
}
