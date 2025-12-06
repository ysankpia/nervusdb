import { describe, it, expect } from 'vitest';
import { MemoryGraph } from '@/extensions/algorithms/graph';
import { GraphAlgorithmSuiteImpl } from '@/extensions/algorithms/suite';

describe('图算法套件 · Centrality and Community Algorithms', () => {
  it('基础调用不抛错（小图）', () => {
    const g = new MemoryGraph();
    g.addNode({ id: '0', value: '0' });
    g.addNode({ id: '1', value: '1' });
    g.addNode({ id: '2', value: '2' });
    g.addEdge({ source: '0', target: '1', type: 'R', weight: 1 });
    g.addEdge({ source: '1', target: '2', type: 'R', weight: 1 });
    g.addEdge({ source: '0', target: '2', type: 'R', weight: 1 });

    const suite = new GraphAlgorithmSuiteImpl(g);
    expect(() => suite.centrality.pageRank()).not.toThrow();
    expect(() => suite.centrality.betweenness()).not.toThrow();
    expect(() => suite.centrality.closeness()).not.toThrow();
    expect(() => suite.centrality.degree()).not.toThrow();
    expect(() => suite.centrality.eigenvector()).not.toThrow();

    expect(() => suite.community.louvain()).not.toThrow();
    expect(() => suite.community.labelPropagation()).not.toThrow();
    expect(() => suite.community.connectedComponents()).not.toThrow();
    expect(() => suite.community.stronglyConnectedComponents()).not.toThrow();
  });
});
