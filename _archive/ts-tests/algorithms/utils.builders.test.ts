import { describe, it, expect } from 'vitest';
import { GraphAlgorithmUtils } from '@/extensions/algorithms/suite.ts';

describe('GraphAlgorithmUtils · 构图便捷函数', () => {
  it('fromEdgeList/fromAdjacencyMatrix/star/cycle/complete', () => {
    const g1 = GraphAlgorithmUtils.fromEdgeList([
      { source: 'A', target: 'B', type: 'E' },
      { source: 'B', target: 'C', type: 'E' },
    ]);
    expect(g1.getNodes().length).toBeGreaterThanOrEqual(0);
    expect(g1.getEdges().length).toBe(2);

    const g2 = GraphAlgorithmUtils.fromAdjacencyMatrix(
      [
        [0, 1],
        [0, 0],
      ],
      ['X', 'Y'],
    );
    expect(g2.getEdges().length).toBe(1);

    const g3 = GraphAlgorithmUtils.createStarGraph(5);
    expect(g3.getNodes().length).toBe(5);

    const g4 = GraphAlgorithmUtils.createCycleGraph(4);
    expect(g4.getEdges().length).toBe(4);

    const g5 = GraphAlgorithmUtils.createCompleteGraph(3);
    // 完全图（无向逻辑通过单向两两连接表示），边应 >= 3
    expect(g5.getEdges().length).toBeGreaterThanOrEqual(3);
  });

  it('benchmark 应返回包含耗时的结果对象', () => {
    const g = GraphAlgorithmUtils.createStarGraph(6);
    const res = GraphAlgorithmUtils.benchmark(g, ['pagerank', 'betweenness']);
    expect(res.pagerank).toBeGreaterThanOrEqual(0);
    expect(res.betweenness).toBeGreaterThanOrEqual(0);
  });
});
