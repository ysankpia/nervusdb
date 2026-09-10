/* tslint:disable */
/* eslint-disable */

export interface DijkstraResult {
  cost: number;
  path: Array<number>;
}

export interface ExecuteResult {
  nodes_created: number;
  edges_created: number;
  nodes_deleted: number;
  edges_deleted: number;
  properties_set: number;
  message: string;
}

export interface BufferStats {
  capacity_frames: number;
  used_frames: number;
  dirty_frames: number;
  cache_hits: number;
  cache_misses: number;
  hit_rate_percentage: number;
  disk_reads: number;
  disk_writes: number;
  file_size_bytes: number;
}

export class GraphLite {
  static open(path: string, poolSize?: number): GraphLite;
  execute(cypher: string): ExecuteResult;
  query(cypher: string): Array<Record<string, any>>;
  addNode(labels: Array<string>, properties?: Record<string, any>): number;
  addEdge(
    src: number,
    dst: number,
    edgeType: string,
    properties?: Record<string, any>,
    weight?: number,
  ): number;
  dijkstra(
    start: number,
    end: number,
    edgeType?: string,
  ): DijkstraResult | null;
  stats(): BufferStats;
  checkpoint(): void;
}
