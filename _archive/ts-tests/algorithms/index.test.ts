import { describe, it, expect } from 'vitest';

// 直接从算法入口导入便捷 API
import {
  createGraph,
  createGraphBuilder,
  createAlgorithmSuite,
} from '@/extensions/algorithms/index.ts';

describe('算法入口与便捷 API', () => {
  it('createGraph 应该返回一个空图实例', () => {
    const g = createGraph();
    expect(g.getNodes()).toEqual([]);
    expect(g.getEdges()).toEqual([]);
  });

  it('createGraphBuilder 应该支持链式构建并生成图', () => {
    const builder = createGraphBuilder();
    builder.addNode('A').addNode('B').addEdge('A', 'B', 'EDGE', 1);
    const g = builder.build();
    expect(
      g
        .getNodes()
        .map((n) => n.id)
        .sort(),
    ).toEqual(['A', 'B']);
    expect(g.getEdges()).toHaveLength(1);
    expect(g.getOutDegree('A')).toBe(1);
    expect(g.getInDegree('B')).toBe(1);
  });

  it('createAlgorithmSuite 应该返回可用的套件并可读取图统计', () => {
    const g = createGraph();
    // 构造一个简单图：A -> B
    const builder = createGraphBuilder();
    const graph = builder.addNode('A').addNode('B').addEdge('A', 'B', 'EDGE', 1).build();

    const suite = createAlgorithmSuite(graph);
    const stats = suite.analysis.getStats();
    expect(stats.nodeCount).toBe(2);
    expect(stats.edgeCount).toBe(1);
    expect(typeof stats.averageDegree).toBe('number');
  });
});
