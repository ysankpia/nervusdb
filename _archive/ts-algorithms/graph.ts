/**
 * 图数据结构实现
 *
 * 提供内存图存储和基础图操作功能
 */

import type { Graph, GraphNode, GraphEdge, GraphStats } from './types.js';

/**
 * 内存图实现
 */
export class MemoryGraph implements Graph {
  private nodes = new Map<string, GraphNode>();
  private edges = new Map<string, GraphEdge>();
  private adjacencyList = new Map<string, Map<string, GraphEdge>>();
  private reverseAdjacencyList = new Map<string, Map<string, GraphEdge>>();

  /**
   * 添加节点
   */
  addNode(node: GraphNode): void {
    this.nodes.set(node.id, { ...node });

    // 初始化邻接表
    if (!this.adjacencyList.has(node.id)) {
      this.adjacencyList.set(node.id, new Map());
    }
    if (!this.reverseAdjacencyList.has(node.id)) {
      this.reverseAdjacencyList.set(node.id, new Map());
    }
  }

  /**
   * 删除节点
   */
  removeNode(nodeId: string): void {
    if (!this.nodes.has(nodeId)) return;

    // 删除与该节点相关的所有边
    const outEdges = this.getOutEdges(nodeId);
    const inEdges = this.getInEdges(nodeId);

    for (const edge of [...outEdges, ...inEdges]) {
      this.removeEdge(edge.source, edge.target);
    }

    // 删除节点
    this.nodes.delete(nodeId);
    this.adjacencyList.delete(nodeId);
    this.reverseAdjacencyList.delete(nodeId);
  }

  /**
   * 添加边
   */
  addEdge(edge: GraphEdge): void {
    const edgeId = this.getEdgeId(edge.source, edge.target);
    const edgeWithDefaults = {
      weight: 1.0,
      directed: true,
      ...edge,
    };

    this.edges.set(edgeId, edgeWithDefaults);

    // 确保节点存在
    if (!this.nodes.has(edge.source)) {
      this.addNode({ id: edge.source, value: edge.source });
    }
    if (!this.nodes.has(edge.target)) {
      this.addNode({ id: edge.target, value: edge.target });
    }

    // 更新邻接表
    if (!this.adjacencyList.has(edge.source)) {
      this.adjacencyList.set(edge.source, new Map());
    }
    this.adjacencyList.get(edge.source)!.set(edge.target, edgeWithDefaults);

    // 更新反向邻接表
    if (!this.reverseAdjacencyList.has(edge.target)) {
      this.reverseAdjacencyList.set(edge.target, new Map());
    }
    this.reverseAdjacencyList.get(edge.target)!.set(edge.source, edgeWithDefaults);

    // 如果是无向边，添加反向边
    if (!edgeWithDefaults.directed) {
      const reverseEdgeId = this.getEdgeId(edge.target, edge.source);
      const reverseEdge = {
        ...edgeWithDefaults,
        source: edge.target,
        target: edge.source,
      };

      this.edges.set(reverseEdgeId, reverseEdge);
      this.adjacencyList.get(edge.target)!.set(edge.source, reverseEdge);
      this.reverseAdjacencyList.get(edge.source)!.set(edge.target, reverseEdge);
    }
  }

  /**
   * 删除边
   */
  removeEdge(source: string, target: string): void {
    const edgeId = this.getEdgeId(source, target);
    const edge = this.edges.get(edgeId);

    if (!edge) return;

    // 删除边
    this.edges.delete(edgeId);

    // 更新邻接表
    const sourceAdjacency = this.adjacencyList.get(source);
    if (sourceAdjacency) {
      sourceAdjacency.delete(target);
    }

    const targetReverse = this.reverseAdjacencyList.get(target);
    if (targetReverse) {
      targetReverse.delete(source);
    }

    // 如果是无向边，删除反向边
    if (!edge.directed) {
      const reverseEdgeId = this.getEdgeId(target, source);
      this.edges.delete(reverseEdgeId);

      const targetAdjacency = this.adjacencyList.get(target);
      if (targetAdjacency) {
        targetAdjacency.delete(source);
      }

      const sourceReverse = this.reverseAdjacencyList.get(source);
      if (sourceReverse) {
        sourceReverse.delete(target);
      }
    }
  }

  /**
   * 获取节点
   */
  getNode(nodeId: string): GraphNode | undefined {
    return this.nodes.get(nodeId);
  }

  /**
   * 获取所有节点
   */
  getNodes(): GraphNode[] {
    return Array.from(this.nodes.values());
  }

  /**
   * 获取节点的邻居
   */
  getNeighbors(nodeId: string): GraphNode[] {
    const adjacency = this.adjacencyList.get(nodeId);
    if (!adjacency) return [];

    return Array.from(adjacency.keys())
      .map((neighborId) => this.nodes.get(neighborId))
      .filter((node): node is GraphNode => node !== undefined);
  }

  /**
   * 获取节点的出边
   */
  getOutEdges(nodeId: string): GraphEdge[] {
    const adjacency = this.adjacencyList.get(nodeId);
    if (!adjacency) return [];

    return Array.from(adjacency.values());
  }

  /**
   * 获取节点的入边
   */
  getInEdges(nodeId: string): GraphEdge[] {
    const reverseAdjacency = this.reverseAdjacencyList.get(nodeId);
    if (!reverseAdjacency) return [];

    return Array.from(reverseAdjacency.values());
  }

  /**
   * 获取所有边
   */
  getEdges(): GraphEdge[] {
    return Array.from(this.edges.values());
  }

  /**
   * 获取节点度数
   */
  getDegree(nodeId: string): number {
    return this.getOutDegree(nodeId) + this.getInDegree(nodeId);
  }

  /**
   * 获取节点出度
   */
  getOutDegree(nodeId: string): number {
    const adjacency = this.adjacencyList.get(nodeId);
    return adjacency ? adjacency.size : 0;
  }

  /**
   * 获取节点入度
   */
  getInDegree(nodeId: string): number {
    const reverseAdjacency = this.reverseAdjacencyList.get(nodeId);
    return reverseAdjacency ? reverseAdjacency.size : 0;
  }

  /**
   * 检查节点是否存在
   */
  hasNode(nodeId: string): boolean {
    return this.nodes.has(nodeId);
  }

  /**
   * 检查边是否存在
   */
  hasEdge(source: string, target: string): boolean {
    return this.edges.has(this.getEdgeId(source, target));
  }

  /**
   * 获取图统计信息
   */
  getStats(): GraphStats {
    const nodeCount = this.nodes.size;
    const edgeCount = this.edges.size;

    let totalDegree = 0;
    for (const nodeId of this.nodes.keys()) {
      totalDegree += this.getDegree(nodeId);
    }

    const averageDegree = nodeCount > 0 ? totalDegree / nodeCount : 0;

    // 计算图密度
    const maxPossibleEdges = nodeCount * (nodeCount - 1);
    const density = maxPossibleEdges > 0 ? edgeCount / maxPossibleEdges : 0;

    // 检查连通性
    const { isConnected, componentCount } = this.analyzeConnectivity();

    return {
      nodeCount,
      edgeCount,
      averageDegree,
      density,
      isConnected,
      componentCount,
    };
  }

  /**
   * 清空图
   */
  clear(): void {
    this.nodes.clear();
    this.edges.clear();
    this.adjacencyList.clear();
    this.reverseAdjacencyList.clear();
  }

  /**
   * 克隆图
   */
  clone(): Graph {
    const newGraph = new MemoryGraph();

    // 克隆节点
    for (const node of this.nodes.values()) {
      newGraph.addNode({
        ...node,
        properties: node.properties ? { ...node.properties } : undefined,
        labels: node.labels ? [...node.labels] : undefined,
      });
    }

    // 克隆边
    for (const edge of this.edges.values()) {
      newGraph.addEdge({
        ...edge,
        properties: edge.properties ? { ...edge.properties } : undefined,
      });
    }

    return newGraph;
  }

  /**
   * 生成边的唯一标识
   */
  private getEdgeId(source: string, target: string): string {
    return `${source}->${target}`;
  }

  /**
   * 分析图的连通性
   */
  private analyzeConnectivity(): { isConnected: boolean; componentCount: number } {
    const visited = new Set<string>();
    let componentCount = 0;

    for (const nodeId of this.nodes.keys()) {
      if (!visited.has(nodeId)) {
        this.dfsVisit(nodeId, visited);
        componentCount++;
      }
    }

    return {
      isConnected: componentCount <= 1,
      componentCount,
    };
  }

  /**
   * 深度优先搜索访问
   */
  private dfsVisit(nodeId: string, visited: Set<string>): void {
    visited.add(nodeId);

    const neighbors = this.getNeighbors(nodeId);
    for (const neighbor of neighbors) {
      if (!visited.has(neighbor.id)) {
        this.dfsVisit(neighbor.id, visited);
      }
    }
  }

  /**
   * 获取子图
   */
  getSubgraph(nodeIds: string[]): Graph {
    const subgraph = new MemoryGraph();
    const nodeSet = new Set(nodeIds);

    // 添加节点
    for (const nodeId of nodeIds) {
      const node = this.nodes.get(nodeId);
      if (node) {
        subgraph.addNode(node);
      }
    }

    // 添加边（只有两端都在子图中的边）
    for (const edge of this.edges.values()) {
      if (nodeSet.has(edge.source) && nodeSet.has(edge.target)) {
        subgraph.addEdge(edge);
      }
    }

    return subgraph;
  }

  /**
   * 获取节点的k跳邻居
   */
  getKHopNeighbors(nodeId: string, k: number): Set<string> {
    if (k <= 0) return new Set();

    const visited = new Set<string>();
    const queue: Array<{ nodeId: string; depth: number }> = [{ nodeId, depth: 0 }];
    let queueIndex = 0;

    visited.add(nodeId);

    while (queueIndex < queue.length) {
      const { nodeId: currentNode, depth } = queue[queueIndex++];

      if (depth < k) {
        const neighbors = this.getNeighbors(currentNode);
        for (const neighbor of neighbors) {
          if (!visited.has(neighbor.id)) {
            visited.add(neighbor.id);
            queue.push({ nodeId: neighbor.id, depth: depth + 1 });
          }
        }
      }
    }

    visited.delete(nodeId); // 移除起始节点
    return visited;
  }

  /**
   * 获取图的直径（最长最短路径）
   */
  getDiameter(): number {
    let maxDistance = 0;

    for (const sourceId of this.nodes.keys()) {
      const distances = this.bfsDistances(sourceId);
      for (const distance of distances.values()) {
        if (distance > maxDistance && distance !== Infinity) {
          maxDistance = distance;
        }
      }
    }

    return maxDistance;
  }

  /**
   * BFS计算单源最短距离
   */
  private bfsDistances(sourceId: string): Map<string, number> {
    const distances = new Map<string, number>();
    const queue: string[] = [sourceId];
    let queueIndex = 0;

    distances.set(sourceId, 0);

    while (queueIndex < queue.length) {
      const currentNode = queue[queueIndex++];
      const currentDistance = distances.get(currentNode)!;

      const neighbors = this.getNeighbors(currentNode);
      for (const neighbor of neighbors) {
        if (!distances.has(neighbor.id)) {
          distances.set(neighbor.id, currentDistance + 1);
          queue.push(neighbor.id);
        }
      }
    }

    return distances;
  }

  /**
   * 计算聚类系数
   */
  getClusteringCoefficient(): number {
    let totalCoefficient = 0;
    let nodeCount = 0;

    for (const nodeId of this.nodes.keys()) {
      const coefficient = this.getNodeClusteringCoefficient(nodeId);
      if (!isNaN(coefficient)) {
        totalCoefficient += coefficient;
        nodeCount++;
      }
    }

    return nodeCount > 0 ? totalCoefficient / nodeCount : 0;
  }

  /**
   * 计算单个节点的聚类系数
   */
  private getNodeClusteringCoefficient(nodeId: string): number {
    const neighbors = this.getNeighbors(nodeId);
    const degree = neighbors.length;

    if (degree < 2) return 0;

    let edgeCount = 0;
    // const neighborSet = new Set(neighbors.map((n) => n.id)); // 未使用

    // 计算邻居间的边数
    for (let i = 0; i < neighbors.length; i++) {
      for (let j = i + 1; j < neighbors.length; j++) {
        if (
          this.hasEdge(neighbors[i].id, neighbors[j].id) ||
          this.hasEdge(neighbors[j].id, neighbors[i].id)
        ) {
          edgeCount++;
        }
      }
    }

    const maxPossibleEdges = (degree * (degree - 1)) / 2;
    return maxPossibleEdges > 0 ? edgeCount / maxPossibleEdges : 0;
  }
}

/**
 * 图构建器
 */
export class GraphBuilder {
  private graph = new MemoryGraph();

  /**
   * 添加节点
   */
  addNode(id: string, value?: string, properties?: Record<string, unknown>): this {
    this.graph.addNode({
      id,
      value: value || id,
      properties,
    });
    return this;
  }

  /**
   * 添加边
   */
  addEdge(
    source: string,
    target: string,
    type?: string,
    weight?: number,
    properties?: Record<string, unknown>,
  ): this {
    this.graph.addEdge({
      source,
      target,
      type: type || 'CONNECTED',
      weight,
      properties,
    });
    return this;
  }

  /**
   * 从邻接矩阵构建图
   */
  fromAdjacencyMatrix(matrix: number[][], nodeIds?: string[]): this {
    const size = matrix.length;
    const ids = nodeIds || Array.from({ length: size }, (_, i) => i.toString());

    // 添加节点
    for (const id of ids) {
      this.addNode(id);
    }

    // 添加边
    for (let i = 0; i < size; i++) {
      for (let j = 0; j < size; j++) {
        if (matrix[i][j] !== 0) {
          this.addEdge(ids[i], ids[j], 'EDGE', matrix[i][j]);
        }
      }
    }

    return this;
  }

  /**
   * 从边列表构建图
   */
  fromEdgeList(edges: Array<{ source: string; target: string; weight?: number }>): this {
    for (const edge of edges) {
      this.addEdge(edge.source, edge.target, 'EDGE', edge.weight);
    }
    return this;
  }

  /**
   * 构建随机图
   */
  random(nodeCount: number, edgeProbability: number): this {
    // 添加节点
    for (let i = 0; i < nodeCount; i++) {
      this.addNode(i.toString());
    }

    // 随机添加边
    for (let i = 0; i < nodeCount; i++) {
      for (let j = i + 1; j < nodeCount; j++) {
        if (Math.random() < edgeProbability) {
          this.addEdge(i.toString(), j.toString(), 'RANDOM');
        }
      }
    }

    return this;
  }

  /**
   * 构建完全图
   */
  complete(nodeCount: number): this {
    // 添加节点
    for (let i = 0; i < nodeCount; i++) {
      this.addNode(i.toString());
    }

    // 添加所有可能的边
    for (let i = 0; i < nodeCount; i++) {
      for (let j = i + 1; j < nodeCount; j++) {
        this.addEdge(i.toString(), j.toString(), 'COMPLETE');
      }
    }

    return this;
  }

  /**
   * 构建星形图
   */
  star(nodeCount: number): this {
    if (nodeCount < 2) return this;

    // 添加中心节点
    this.addNode('0');

    // 添加外围节点和边
    for (let i = 1; i < nodeCount; i++) {
      this.addNode(i.toString());
      this.addEdge('0', i.toString(), 'STAR');
    }

    return this;
  }

  /**
   * 构建环形图
   */
  cycle(nodeCount: number): this {
    if (nodeCount < 3) return this;

    // 添加节点
    for (let i = 0; i < nodeCount; i++) {
      this.addNode(i.toString());
    }

    // 添加环形边
    for (let i = 0; i < nodeCount; i++) {
      const next = (i + 1) % nodeCount;
      this.addEdge(i.toString(), next.toString(), 'CYCLE');
    }

    return this;
  }

  /**
   * 获取构建的图
   */
  build(): Graph {
    return this.graph;
  }
}
