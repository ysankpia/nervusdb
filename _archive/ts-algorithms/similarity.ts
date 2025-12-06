/**
 * 相似度计算算法实现
 *
 * 提供各种图节点相似度计算方法，包括结构相似度和语义相似度
 */

import type { Graph, SimilarityAlgorithm, SimilarityResult } from './types.js';

/**
 * Jaccard相似度算法实现
 * 基于共同邻居的集合相似度计算
 */
export class JaccardSimilarity implements SimilarityAlgorithm {
  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    if (node1 === node2) return 1.0;

    const neighbors1 = new Set(graph.getNeighbors(node1).map((n) => n.id));
    const neighbors2 = new Set(graph.getNeighbors(node2).map((n) => n.id));

    // 计算交集大小
    const intersection = new Set([...neighbors1].filter((n) => neighbors2.has(n)));

    // 计算并集大小
    const union = new Set([...neighbors1, ...neighbors2]);

    if (union.size === 0) return 0;

    return intersection.size / union.size;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const nodes = graph.getNodes();
    const similarities = new Map<string, Map<string, number>>();
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    // 初始化相似度矩阵
    nodes.forEach((node) => {
      similarities.set(node.id, new Map());
    });

    // 计算所有节点对的相似度
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const node1 = nodes[i];
        const node2 = nodes[j];
        const similarity = this.computeSimilarity(graph, node1.id, node2.id);

        similarities.get(node1.id)!.set(node2.id, similarity);
        similarities.get(node2.id)!.set(node1.id, similarity);

        if (similarity >= threshold) {
          topPairs.push({ node1: node1.id, node2: node2.id, similarity });
        }
      }
    }

    // 按相似度降序排序
    topPairs.sort((a, b) => b.similarity - a.similarity);

    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const nodes = graph.getNodes();
    const similarities: Array<{ nodeId: string; similarity: number }> = [];

    for (const node of nodes) {
      if (node.id !== targetNode) {
        const similarity = this.computeSimilarity(graph, targetNode, node.id);
        similarities.push({ nodeId: node.id, similarity });
      }
    }

    // 按相似度降序排序并返回前k个
    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}

/**
 * 余弦相似度算法实现
 * 基于节点度数向量的余弦相似度
 */
export class CosineSimilarity implements SimilarityAlgorithm {
  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    if (node1 === node2) return 1.0;

    // 获取所有可能的谓词（边类型）
    const allPredicates = new Set<string>();
    graph.getEdges().forEach((edge) => allPredicates.add(edge.type));

    // 构建特征向量（基于每种谓词类型的度数）
    const vector1 = this.buildFeatureVector(graph, node1, allPredicates);
    const vector2 = this.buildFeatureVector(graph, node2, allPredicates);

    return this.cosineSimilarity(vector1, vector2);
  }

  private buildFeatureVector(graph: Graph, nodeId: string, predicates: Set<string>): number[] {
    const vector: number[] = [];

    predicates.forEach((predicate) => {
      // 计算该节点在特定谓词类型上的度数
      const outEdges = graph.getOutEdges(nodeId).filter((e) => e.type === predicate);
      const inEdges = graph.getInEdges(nodeId).filter((e) => e.type === predicate);
      vector.push(outEdges.length + inEdges.length);
    });

    return vector;
  }

  private cosineSimilarity(vector1: number[], vector2: number[]): number {
    if (vector1.length !== vector2.length) return 0;

    let dotProduct = 0;
    let norm1 = 0;
    let norm2 = 0;

    for (let i = 0; i < vector1.length; i++) {
      dotProduct += vector1[i] * vector2[i];
      norm1 += vector1[i] * vector1[i];
      norm2 += vector2[i] * vector2[i];
    }

    const magnitude = Math.sqrt(norm1) * Math.sqrt(norm2);
    return magnitude === 0 ? 0 : dotProduct / magnitude;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const nodes = graph.getNodes();
    const similarities = new Map<string, Map<string, number>>();
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    // 预计算所有谓词类型
    const allPredicates = new Set<string>();
    graph.getEdges().forEach((edge) => allPredicates.add(edge.type));

    // 预计算所有节点的特征向量
    const featureVectors = new Map<string, number[]>();
    nodes.forEach((node) => {
      featureVectors.set(node.id, this.buildFeatureVector(graph, node.id, allPredicates));
    });

    // 初始化相似度矩阵
    nodes.forEach((node) => {
      similarities.set(node.id, new Map());
    });

    // 计算所有节点对的相似度
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const node1 = nodes[i];
        const node2 = nodes[j];
        const vector1 = featureVectors.get(node1.id)!;
        const vector2 = featureVectors.get(node2.id)!;
        const similarity = this.cosineSimilarity(vector1, vector2);

        similarities.get(node1.id)!.set(node2.id, similarity);
        similarities.get(node2.id)!.set(node1.id, similarity);

        if (similarity >= threshold) {
          topPairs.push({ node1: node1.id, node2: node2.id, similarity });
        }
      }
    }

    // 按相似度降序排序
    topPairs.sort((a, b) => b.similarity - a.similarity);

    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const nodes = graph.getNodes();
    const similarities: Array<{ nodeId: string; similarity: number }> = [];

    for (const node of nodes) {
      if (node.id !== targetNode) {
        const similarity = this.computeSimilarity(graph, targetNode, node.id);
        similarities.push({ nodeId: node.id, similarity });
      }
    }

    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}

/**
 * Adamic-Adar相似度算法实现
 * 基于共同邻居的重要性加权相似度
 */
export class AdamicAdarSimilarity implements SimilarityAlgorithm {
  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    if (node1 === node2) return 1.0;

    const neighbors1 = new Set(graph.getNeighbors(node1).map((n) => n.id));
    const neighbors2 = new Set(graph.getNeighbors(node2).map((n) => n.id));

    // 找到共同邻居
    const commonNeighbors = [...neighbors1].filter((n) => neighbors2.has(n));

    if (commonNeighbors.length === 0) return 0;

    // 计算Adamic-Adar指数
    let score = 0;
    for (const neighbor of commonNeighbors) {
      const neighborDegree = graph.getDegree(neighbor);
      if (neighborDegree > 1) {
        score += 1 / Math.log(neighborDegree);
      }
    }

    return score;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const nodes = graph.getNodes();
    const similarities = new Map<string, Map<string, number>>();
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    // 初始化相似度矩阵
    nodes.forEach((node) => {
      similarities.set(node.id, new Map());
    });

    // 计算所有节点对的相似度
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const node1 = nodes[i];
        const node2 = nodes[j];
        const similarity = this.computeSimilarity(graph, node1.id, node2.id);

        similarities.get(node1.id)!.set(node2.id, similarity);
        similarities.get(node2.id)!.set(node1.id, similarity);

        if (similarity >= threshold) {
          topPairs.push({ node1: node1.id, node2: node2.id, similarity });
        }
      }
    }

    topPairs.sort((a, b) => b.similarity - a.similarity);
    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const nodes = graph.getNodes();
    const similarities: Array<{ nodeId: string; similarity: number }> = [];

    for (const node of nodes) {
      if (node.id !== targetNode) {
        const similarity = this.computeSimilarity(graph, targetNode, node.id);
        similarities.push({ nodeId: node.id, similarity });
      }
    }

    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}

/**
 * 优先连接相似度算法实现
 * 基于度数乘积的相似度计算
 */
export class PreferentialAttachmentSimilarity implements SimilarityAlgorithm {
  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    if (node1 === node2) return 1.0;

    const degree1 = graph.getDegree(node1);
    const degree2 = graph.getDegree(node2);

    // 优先连接指数：度数的乘积
    return degree1 * degree2;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const nodes = graph.getNodes();
    const similarities = new Map<string, Map<string, number>>();
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    // 初始化相似度矩阵
    nodes.forEach((node) => {
      similarities.set(node.id, new Map());
    });

    // 计算所有节点对的相似度
    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const node1 = nodes[i];
        const node2 = nodes[j];
        const similarity = this.computeSimilarity(graph, node1.id, node2.id);

        similarities.get(node1.id)!.set(node2.id, similarity);
        similarities.get(node2.id)!.set(node1.id, similarity);

        if (similarity >= threshold) {
          topPairs.push({ node1: node1.id, node2: node2.id, similarity });
        }
      }
    }

    topPairs.sort((a, b) => b.similarity - a.similarity);
    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const nodes = graph.getNodes();
    const similarities: Array<{ nodeId: string; similarity: number }> = [];

    for (const node of nodes) {
      if (node.id !== targetNode) {
        const similarity = this.computeSimilarity(graph, targetNode, node.id);
        similarities.push({ nodeId: node.id, similarity });
      }
    }

    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}

/**
 * SimRank相似度算法实现
 * 基于"相似的对象被相似的对象关联"的原理
 */
export class SimRankSimilarity implements SimilarityAlgorithm {
  private dampingFactor: number;
  private maxIterations: number;
  private tolerance: number;

  constructor(
    options: { dampingFactor?: number; maxIterations?: number; tolerance?: number } = {},
  ) {
    this.dampingFactor = options.dampingFactor ?? 0.8;
    this.maxIterations = options.maxIterations ?? 100;
    this.tolerance = options.tolerance ?? 1e-6;
  }

  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    if (node1 === node2) return 1.0;

    const allSimilarities = this.computeAllSimRankSimilarities(graph);
    const node1Similarities = allSimilarities.get(node1);
    return node1Similarities?.get(node2) ?? 0;
  }

  private computeAllSimRankSimilarities(graph: Graph): Map<string, Map<string, number>> {
    const nodes = graph.getNodes();
    const nodeIds = nodes.map((n) => n.id);
    const n = nodeIds.length;

    // 初始化相似度矩阵
    let similarities = new Map<string, Map<string, number>>();
    let newSimilarities = new Map<string, Map<string, number>>();

    nodeIds.forEach((nodeId) => {
      similarities.set(nodeId, new Map());
      newSimilarities.set(nodeId, new Map());

      nodeIds.forEach((otherId) => {
        const initialSim = nodeId === otherId ? 1.0 : 0.0;
        similarities.get(nodeId)!.set(otherId, initialSim);
        newSimilarities.get(nodeId)!.set(otherId, initialSim);
      });
    });

    // 迭代计算SimRank
    for (let iteration = 0; iteration < this.maxIterations; iteration++) {
      let maxChange = 0;

      for (let i = 0; i < n; i++) {
        for (let j = i + 1; j < n; j++) {
          const nodeI = nodeIds[i];
          const nodeJ = nodeIds[j];

          if (nodeI === nodeJ) {
            newSimilarities.get(nodeI)!.set(nodeJ, 1.0);
            continue;
          }

          // 获取入邻居
          const inNeighborsI = graph.getInEdges(nodeI).map((e) => e.source);
          const inNeighborsJ = graph.getInEdges(nodeJ).map((e) => e.source);

          if (inNeighborsI.length === 0 || inNeighborsJ.length === 0) {
            newSimilarities.get(nodeI)!.set(nodeJ, 0);
            newSimilarities.get(nodeJ)!.set(nodeI, 0);
            continue;
          }

          // 计算SimRank相似度
          let sum = 0;
          for (const neighborI of inNeighborsI) {
            for (const neighborJ of inNeighborsJ) {
              sum += similarities.get(neighborI)!.get(neighborJ) ?? 0;
            }
          }

          const newSim = (this.dampingFactor * sum) / (inNeighborsI.length * inNeighborsJ.length);
          newSimilarities.get(nodeI)!.set(nodeJ, newSim);
          newSimilarities.get(nodeJ)!.set(nodeI, newSim);

          // 跟踪最大变化
          const oldSim = similarities.get(nodeI)!.get(nodeJ) ?? 0;
          maxChange = Math.max(maxChange, Math.abs(newSim - oldSim));
        }
      }

      // 交换矩阵
      [similarities, newSimilarities] = [newSimilarities, similarities];

      // 检查收敛
      if (maxChange < this.tolerance) {
        break;
      }
    }

    return similarities;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const similarities = this.computeAllSimRankSimilarities(graph);
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    similarities.forEach((nodeMap, node1) => {
      nodeMap.forEach((similarity, node2) => {
        if (node1 < node2 && similarity >= threshold) {
          topPairs.push({ node1, node2, similarity });
        }
      });
    });

    topPairs.sort((a, b) => b.similarity - a.similarity);
    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const allSimilarities = this.computeAllSimRankSimilarities(graph);
    const targetSimilarities = allSimilarities.get(targetNode);

    if (!targetSimilarities) return [];

    const similarities: Array<{ nodeId: string; similarity: number }> = [];
    targetSimilarities.forEach((similarity, nodeId) => {
      if (nodeId !== targetNode) {
        similarities.push({ nodeId, similarity });
      }
    });

    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}

/**
 * 节点属性相似度算法实现
 * 基于节点属性的语义相似度计算
 */
export class NodeAttributeSimilarity implements SimilarityAlgorithm {
  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    if (node1 === node2) return 1.0;

    const nodeObj1 = graph.getNode(node1);
    const nodeObj2 = graph.getNode(node2);

    if (!nodeObj1 || !nodeObj2) return 0;

    // 如果没有属性，基于节点值计算
    if (!nodeObj1.properties && !nodeObj2.properties) {
      return this.stringSimilarity(nodeObj1.value, nodeObj2.value);
    }

    // 基于属性计算相似度
    let props1: Record<string, unknown> = {};
    let props2: Record<string, unknown> = {};
    if (nodeObj1.properties) props1 = nodeObj1.properties;
    if (nodeObj2.properties) props2 = nodeObj2.properties;

    const allKeys = new Set([...Object.keys(props1), ...Object.keys(props2)]);
    if (allKeys.size === 0) return 0;

    let totalSimilarity = 0;
    let validComparisons = 0;

    for (const key of allKeys) {
      const val1 = props1[key];
      const val2 = props2[key];

      if (val1 !== undefined && val2 !== undefined) {
        totalSimilarity += this.valueSimilarity(val1, val2);
        validComparisons++;
      }
    }

    return validComparisons > 0 ? totalSimilarity / validComparisons : 0;
  }

  private stringSimilarity(str1: string, str2: string): number {
    // 简单的编辑距离相似度
    const maxLen = Math.max(str1.length, str2.length);
    if (maxLen === 0) return 1.0;

    const distance = this.levenshteinDistance(str1, str2);
    return 1 - distance / maxLen;
  }

  private levenshteinDistance(str1: string, str2: string): number {
    const rows = str2.length + 1;
    const cols = str1.length + 1;
    const matrix: number[][] = Array.from({ length: rows }, () => new Array<number>(cols).fill(0));

    for (let i = 0; i <= str1.length; i++) matrix[0][i] = i;
    for (let j = 0; j <= str2.length; j++) matrix[j][0] = j;

    for (let j = 1; j <= str2.length; j++) {
      for (let i = 1; i <= str1.length; i++) {
        const indicator = str1[i - 1] === str2[j - 1] ? 0 : 1;
        matrix[j][i] = Math.min(
          matrix[j][i - 1] + 1,
          matrix[j - 1][i] + 1,
          matrix[j - 1][i - 1] + indicator,
        );
      }
    }

    return matrix[str2.length][str1.length];
  }

  private valueSimilarity(val1: unknown, val2: unknown): number {
    if (val1 === val2) return 1.0;

    const type1 = typeof val1;
    const type2 = typeof val2;

    if (type1 !== type2) return 0;

    if (type1 === 'string' && type2 === 'string') {
      return this.stringSimilarity(val1 as string, val2 as string);
    }

    if (type1 === 'number' && type2 === 'number') {
      const v1 = val1 as number;
      const v2 = val2 as number;
      const maxVal = Math.max(Math.abs(v1), Math.abs(v2));
      if (maxVal === 0) return 1.0;
      return 1 - Math.abs(v1 - v2) / maxVal;
    }

    if (Array.isArray(val1) && Array.isArray(val2)) {
      return this.arraySimilarity(val1 as unknown[], val2 as unknown[]);
    }

    return 0;
  }

  private arraySimilarity(arr1: unknown[], arr2: unknown[]): number {
    const set1 = new Set(arr1);
    const set2 = new Set(arr2);
    const intersection = new Set([...set1].filter((x) => set2.has(x)));
    const union = new Set([...set1, ...set2]);

    return union.size > 0 ? intersection.size / union.size : 0;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const nodes = graph.getNodes();
    const similarities = new Map<string, Map<string, number>>();
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    nodes.forEach((node) => {
      similarities.set(node.id, new Map());
    });

    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const node1 = nodes[i];
        const node2 = nodes[j];
        const similarity = this.computeSimilarity(graph, node1.id, node2.id);

        similarities.get(node1.id)!.set(node2.id, similarity);
        similarities.get(node2.id)!.set(node1.id, similarity);

        if (similarity >= threshold) {
          topPairs.push({ node1: node1.id, node2: node2.id, similarity });
        }
      }
    }

    topPairs.sort((a, b) => b.similarity - a.similarity);
    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const nodes = graph.getNodes();
    const similarities: Array<{ nodeId: string; similarity: number }> = [];

    for (const node of nodes) {
      if (node.id !== targetNode) {
        const similarity = this.computeSimilarity(graph, targetNode, node.id);
        similarities.push({ nodeId: node.id, similarity });
      }
    }

    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}

/**
 * 相似度算法工厂
 */
export class SimilarityAlgorithmFactory {
  /**
   * 创建Jaccard相似度算法实例
   */
  static createJaccard(): JaccardSimilarity {
    return new JaccardSimilarity();
  }

  /**
   * 创建余弦相似度算法实例
   */
  static createCosine(): CosineSimilarity {
    return new CosineSimilarity();
  }

  /**
   * 创建Adamic-Adar相似度算法实例
   */
  static createAdamic(): AdamicAdarSimilarity {
    return new AdamicAdarSimilarity();
  }

  /**
   * 创建优先连接相似度算法实例
   */
  static createPreferentialAttachment(): PreferentialAttachmentSimilarity {
    return new PreferentialAttachmentSimilarity();
  }

  /**
   * 创建SimRank相似度算法实例
   */
  static createSimRank(options?: {
    dampingFactor?: number;
    maxIterations?: number;
    tolerance?: number;
  }): SimRankSimilarity {
    return new SimRankSimilarity(options);
  }

  /**
   * 创建节点属性相似度算法实例
   */
  static createNodeAttribute(): NodeAttributeSimilarity {
    return new NodeAttributeSimilarity();
  }

  /**
   * 根据类型创建算法实例
   */
  static create(
    type:
      | 'jaccard'
      | 'cosine'
      | 'adamic'
      | 'preferential_attachment'
      | 'simrank'
      | 'node_attribute',
  ): SimilarityAlgorithm {
    switch (type) {
      case 'jaccard':
        return this.createJaccard();
      case 'cosine':
        return this.createCosine();
      case 'adamic':
        return this.createAdamic();
      case 'preferential_attachment':
        return this.createPreferentialAttachment();
      case 'simrank':
        return this.createSimRank();
      case 'node_attribute':
        return this.createNodeAttribute();
      default:
        // 理论上不可达，类型已穷尽
        throw new Error('未知的相似度算法类型');
    }
  }

  /**
   * 创建组合相似度算法，结合多种相似度计算方法
   */
  static createComposite(
    algorithms: Array<{ algorithm: SimilarityAlgorithm; weight: number }>,
  ): SimilarityAlgorithm {
    return new CompositeSimilarityAlgorithm(algorithms);
  }
}

/**
 * 组合相似度算法实现
 * 结合多种相似度算法的加权结果
 */
class CompositeSimilarityAlgorithm implements SimilarityAlgorithm {
  constructor(private algorithms: Array<{ algorithm: SimilarityAlgorithm; weight: number }>) {}

  computeSimilarity(graph: Graph, node1: string, node2: string): number {
    let totalScore = 0;
    let totalWeight = 0;

    for (const { algorithm, weight } of this.algorithms) {
      const score = algorithm.computeSimilarity(graph, node1, node2);
      totalScore += score * weight;
      totalWeight += weight;
    }

    return totalWeight > 0 ? totalScore / totalWeight : 0;
  }

  computeAllSimilarities(graph: Graph, threshold: number = 0): SimilarityResult {
    const nodes = graph.getNodes();
    const similarities = new Map<string, Map<string, number>>();
    const topPairs: Array<{ node1: string; node2: string; similarity: number }> = [];

    nodes.forEach((node) => {
      similarities.set(node.id, new Map());
    });

    for (let i = 0; i < nodes.length; i++) {
      for (let j = i + 1; j < nodes.length; j++) {
        const node1 = nodes[i];
        const node2 = nodes[j];
        const similarity = this.computeSimilarity(graph, node1.id, node2.id);

        similarities.get(node1.id)!.set(node2.id, similarity);
        similarities.get(node2.id)!.set(node1.id, similarity);

        if (similarity >= threshold) {
          topPairs.push({ node1: node1.id, node2: node2.id, similarity });
        }
      }
    }

    topPairs.sort((a, b) => b.similarity - a.similarity);
    return { similarities, topPairs };
  }

  findMostSimilar(
    graph: Graph,
    targetNode: string,
    k: number,
  ): Array<{ nodeId: string; similarity: number }> {
    const nodes = graph.getNodes();
    const similarities: Array<{ nodeId: string; similarity: number }> = [];

    for (const node of nodes) {
      if (node.id !== targetNode) {
        const similarity = this.computeSimilarity(graph, targetNode, node.id);
        similarities.push({ nodeId: node.id, similarity });
      }
    }

    return similarities.sort((a, b) => b.similarity - a.similarity).slice(0, k);
  }
}
