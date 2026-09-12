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
  wal_page_count: number;
  spill_count: number;
  wal_size_bytes: number;
  wal_fsync_count: number;
  wal_frames_written: number;
}

export interface PageRankScore {
  node_id: number;
  score: number;
}

export interface SubgraphEdge {
  id: number;
  src_id: number;
  dst_id: number;
  edge_type: string;
  weight: number;
}

export interface KHopSubgraph {
  nodes: Array<number>;
  edges: Array<SubgraphEdge>;
}

export class NervusDb {
  static open(path: string, poolSize?: number): NervusDb;

  /** 执行 Cypher 变更语句，返回执行摘要 */
  execute(cypher: string): ExecuteResult;

  /** 执行 Cypher 查询，返回结构化行对象数组 */
  query(cypher: string): Array<Record<string, any>>;

  addNode(labels: Array<string>, properties?: Record<string, any>): number;

  addEdge(
    src: number,
    dst: number,
    edgeType: string,
    properties?: Record<string, any>,
    weight?: number,
  ): number;

  /** 带权 Dijkstra 最短路径 */
  dijkstra(
    start: number,
    end: number,
    edgeType?: string,
  ): DijkstraResult | null;

  /** 无权 BFS 最短路径（最少跳数） */
  bfs(start: number, end: number, edgeType?: string): Array<number> | null;

  /** 有向环路检测 */
  hasCycle(): boolean;

  /** 弱连通分量（按分量规模降序） */
  weaklyConnectedComponents(): Array<Array<number>>;

  /** PageRank 阻尼迭代（按分数降序） */
  pageRank(
    dampingFactor?: number,
    maxIterations?: number,
    tolerance?: number,
  ): Array<PageRankScore>;

  /** K-Hop 局部子图提取（direction: "outgoing" | "incoming" | "both"） */
  kHopSubgraph(
    start: number,
    k: number,
    direction?: string,
    edgeType?: string,
  ): KHopSubgraph;

  /** 全部节点标签 */
  labels(): Array<string>;

  /** 全部关系类型 */
  edgeTypes(): Array<string>;

  /** 导出为可回灌的 Cypher 脚本 */
  dumpCypher(): string;

  /** 开启显式批量事务（脏页在 commit() 时批量写 WAL 并只做一次 fsync） */
  beginTransaction(): Transaction;

  stats(): BufferStats;

  checkpoint(): void;
}

/** 显式批量事务句柄：所有变更在 commit() 时原子提交 */
export class Transaction {
  txId(): number;
  addNode(labels: Array<string>, properties?: Record<string, any>): number;
  addEdge(
    src: number,
    dst: number,
    edgeType: string,
    properties?: Record<string, any>,
    weight?: number,
  ): number;
  updateNodeProperty(nodeId: number, key: string, value: any): void;
  updateEdgeProperty(edgeId: number, key: string, value: any): void;
  removeNode(nodeId: number): void;
  removeEdge(edgeId: number): void;
  commit(): void;
  rollback(): void;
}
