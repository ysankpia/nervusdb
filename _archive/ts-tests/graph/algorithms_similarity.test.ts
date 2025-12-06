import { describe, it, expect } from 'vitest';
import { GraphBuilder } from '@/extensions/algorithms/graph';
import {
  JaccardSimilarity,
  CosineSimilarity,
  AdamicAdarSimilarity,
  PreferentialAttachmentSimilarity,
  NodeAttributeSimilarity,
  SimilarityAlgorithmFactory,
} from '@/extensions/algorithms/similarity';

describe('图算法 · 相似度（Jaccard/Cosine/Adamic/PA/属性）', () => {
  it('Jaccard：共同邻居/并集计算应正确', () => {
    // A 邻居：X,Y; B 邻居：Y,Z => 交集1，并集3 => 1/3
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('X')
      .addNode('Y')
      .addNode('Z')
      .addEdge('A', 'X', 'R')
      .addEdge('A', 'Y', 'R')
      .addEdge('B', 'Y', 'R')
      .addEdge('B', 'Z', 'R')
      .build();
    const s = new JaccardSimilarity();
    expect(s.computeSimilarity(g, 'A', 'B')).toBeCloseTo(1 / 3, 5);
  });

  it('Cosine：基于不同谓词类型的度数向量计算', () => {
    // A: 2条 FRIEND + 0条 LIKE；B: 1条 FRIEND + 1条 LIKE
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addNode('D')
      .addEdge('A', 'C', 'FRIEND')
      .addEdge('A', 'D', 'FRIEND')
      .addEdge('B', 'C', 'FRIEND')
      .addEdge('B', 'D', 'LIKE')
      .build();
    const s = new CosineSimilarity();
    const val = s.computeSimilarity(g, 'A', 'B');
    expect(val).toBeGreaterThan(0);
    expect(val).toBeLessThanOrEqual(1);
  });

  it('Adamic-Adar：对共同邻居按 1/log(deg) 加权', () => {
    // 共同邻居 Y；设置其度数>1 以产生正分数
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('Y')
      .addNode('T')
      .addEdge('A', 'Y', 'R')
      .addEdge('B', 'Y', 'R')
      .addEdge('Y', 'T', 'R')
      .build();
    const s = new AdamicAdarSimilarity();
    const val = s.computeSimilarity(g, 'A', 'B');
    expect(val).toBeGreaterThan(0);
  });

  it('Preferential Attachment：返回度数乘积', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('X')
      .addNode('Y')
      .addEdge('A', 'X', 'R')
      .addEdge('A', 'Y', 'R') // deg(A)=2
      .addEdge('B', 'X', 'R') // deg(B)=1
      .build();
    const s = new PreferentialAttachmentSimilarity();
    expect(s.computeSimilarity(g, 'A', 'B')).toBe(2 * 1);
  });

  it('节点属性相似度：多类型（字符串/数字/数组）应合理比较', () => {
    const g = new GraphBuilder()
      .addNode('U1', 'U1', { name: 'alice', age: 30, tags: ['a', 'b'] })
      .addNode('U2', 'U2', { name: 'alic', age: 29, tags: ['a', 'c'] })
      .addEdge('U1', 'U2', 'KNOWS')
      .build();
    const s = new NodeAttributeSimilarity();
    const val = s.computeSimilarity(g, 'U1', 'U2');
    expect(val).toBeGreaterThan(0);
    expect(val).toBeLessThanOrEqual(1);
  });

  it('工厂：应能创建主流相似度算法', () => {
    expect(() => SimilarityAlgorithmFactory.create('jaccard')).not.toThrow();
    expect(() => SimilarityAlgorithmFactory.create('cosine')).not.toThrow();
    expect(() => SimilarityAlgorithmFactory.create('adamic')).not.toThrow();
    expect(() => SimilarityAlgorithmFactory.create('preferential_attachment')).not.toThrow();
    expect(() => SimilarityAlgorithmFactory.create('simrank')).not.toThrow();
    expect(() => SimilarityAlgorithmFactory.create('node_attribute')).not.toThrow();
  });
});
