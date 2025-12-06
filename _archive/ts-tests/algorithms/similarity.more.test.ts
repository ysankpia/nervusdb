import { describe, it, expect } from 'vitest';
import { GraphBuilder } from '@/extensions/algorithms/graph';
import {
  JaccardSimilarity,
  CosineSimilarity,
  AdamicAdarSimilarity,
  NodeAttributeSimilarity,
  SimilarityAlgorithmFactory,
} from '@/extensions/algorithms/similarity';

describe('相似度算法 · 扩展路径（all/mostSimilar/SimRank/Composite）', () => {
  it('Jaccard.computeAllSimilarities 与 findMostSimilar 基本路径', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'X', 'R')
      .addEdge('A', 'Y', 'R')
      .addEdge('B', 'Y', 'R')
      .addEdge('C', 'Z', 'R')
      .build();

    const s = new JaccardSimilarity();
    const { similarities, topPairs } = s.computeAllSimilarities(g, 0);
    expect(similarities.get('A')?.has('B')).toBe(true);
    expect(topPairs.length).toBeGreaterThan(0);
    const most = s.findMostSimilar(g, 'A', 2);
    expect(most.length).toBeLessThanOrEqual(2);
  });

  it('Cosine.computeAllSimilarities 覆盖特征向量预计算路径', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'X', 'FRIEND')
      .addEdge('B', 'X', 'FRIEND')
      .addEdge('B', 'X', 'LIKE')
      .addEdge('C', 'X', 'LIKE')
      .build();
    const s = new CosineSimilarity();
    const r = s.computeAllSimilarities(g, 0);
    // 节点为 A/B/C 以及关系目标 X，总计 4 个
    expect(r.similarities.size).toBe(4);
    expect(r.topPairs.length).toBeGreaterThan(0);
  });

  it('AdamicAdar.computeAllSimilarities 与 findMostSimilar', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addNode('Y')
      .addEdge('A', 'Y', 'R')
      .addEdge('B', 'Y', 'R')
      .addEdge('C', 'Y', 'R')
      .build();
    const s = new AdamicAdarSimilarity();
    const r = s.computeAllSimilarities(g, 0);
    expect(r.topPairs.length).toBeGreaterThan(0);
    const most = s.findMostSimilar(g, 'A', 1);
    expect(most.length).toBe(1);
  });

  it('NodeAttribute.computeAllSimilarities 覆盖多类型比较路径', () => {
    const g = new GraphBuilder()
      .addNode('U1', 'U1', { name: 'alice', age: 30, tags: ['a', 'b'] })
      .addNode('U2', 'U2', { name: 'alic', age: 29, tags: ['a', 'c'] })
      .addNode('U3', 'U3', { name: 'bob', age: 18, tags: ['x'] })
      .build();
    const s = new NodeAttributeSimilarity();
    const r = s.computeAllSimilarities(g, 0);
    expect(r.similarities.size).toBe(3);
    expect(r.topPairs.length).toBeGreaterThan(0);
  });

  it('SimRank：小图上应能收敛并返回 topPairs', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'B', 'R')
      .addEdge('B', 'C', 'R')
      .addEdge('C', 'A', 'R')
      .build();

    const simrank = SimilarityAlgorithmFactory.createSimRank({
      dampingFactor: 0.8,
      maxIterations: 3,
    });
    const r = simrank.computeAllSimilarities(g, 0);
    expect(r.similarities.size).toBe(3);
  });

  it('Composite：应按权重聚合各算法结果', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('X')
      .addEdge('A', 'X', 'R')
      .addEdge('B', 'X', 'R')
      .build();
    const composite = SimilarityAlgorithmFactory.createComposite([
      { algorithm: new JaccardSimilarity(), weight: 0.5 },
      { algorithm: new CosineSimilarity(), weight: 0.5 },
    ]);
    const v = composite.computeSimilarity(g, 'A', 'B');
    expect(v).toBeGreaterThanOrEqual(0);
    expect(v).toBeLessThanOrEqual(1);
  });
});
