import { PersistentStore, FactRecord } from '../../storage/persistentStore.js';

export type Uniqueness = 'NODE' | 'EDGE' | 'NONE';
export type Direction = 'forward' | 'reverse';

export interface VariablePathOptions {
  min?: number;
  max: number;
  uniqueness?: Uniqueness;
  direction?: Direction;
}

export interface PathEdge {
  record: FactRecord;
  direction: Direction;
}

export interface PathResult {
  edges: PathEdge[];
  length: number;
  startId: number;
  endId: number;
}

export class VariablePathBuilder {
  constructor(
    private readonly store: PersistentStore,
    private readonly startNodes: Set<number>,
    private readonly predicateId: number,
    private readonly options: VariablePathOptions,
  ) {}

  private neighbors(nodeId: number, dir: Direction): FactRecord[] {
    const crit =
      dir === 'forward'
        ? { subjectId: nodeId, predicateId: this.predicateId }
        : { predicateId: this.predicateId, objectId: nodeId };
    const enc = this.store.query(crit);
    return this.store.resolveRecords(enc);
  }

  private nextNode(rec: FactRecord, from: number, dir: Direction): number {
    if (dir === 'forward') return rec.objectId;
    // reverse
    return rec.subjectId;
  }

  all(target?: number): PathResult[] {
    const min = Math.max(1, this.options.min ?? 1);
    const max = Math.max(min, this.options.max);
    const dir = this.options.direction ?? 'forward';
    const uniq = this.options.uniqueness ?? 'NODE';

    const results: PathResult[] = [];
    type Item = {
      node: number;
      edges: PathEdge[];
      visitedNodes: Set<number>;
      visitedEdges: Set<string>;
    };

    const queue: Item[] = [];
    for (const s of this.startNodes) {
      queue.push({ node: s, edges: [], visitedNodes: new Set([s]), visitedEdges: new Set() });
    }

    while (queue.length > 0) {
      const cur = queue.shift()!;
      const depth = cur.edges.length;
      if (depth >= min) {
        if (target === undefined || cur.node === target) {
          results.push({
            edges: cur.edges,
            length: cur.edges.length,
            startId: cur.edges[0]?.record.subjectId ?? cur.node,
            endId: cur.node,
          });
        }
      }
      if (depth >= max) continue;

      for (const rec of this.neighbors(cur.node, dir)) {
        const next = this.nextNode(rec, cur.node, dir);
        const edgeKey = `${rec.subjectId}:${rec.predicateId}:${rec.objectId}`;
        if (uniq === 'NODE' && cur.visitedNodes.has(next)) continue;
        if (uniq === 'EDGE' && cur.visitedEdges.has(edgeKey)) continue;
        const nextEdges = [...cur.edges, { record: rec, direction: dir }];
        const nextVisitedNodes = new Set(cur.visitedNodes);
        nextVisitedNodes.add(next);
        const nextVisitedEdges = new Set(cur.visitedEdges);
        nextVisitedEdges.add(edgeKey);
        queue.push({
          node: next,
          edges: nextEdges,
          visitedNodes: nextVisitedNodes,
          visitedEdges: nextVisitedEdges,
        });
      }
    }

    return results;
  }

  shortest(target: number): PathResult | null {
    const res = this.all(target);
    if (res.length === 0) return null;
    res.sort((a, b) => a.length - b.length);
    return res[0] ?? null;
  }
}
