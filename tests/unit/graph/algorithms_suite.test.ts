import { describe, it, expect } from 'vitest';
import { MemoryGraph } from '@/algorithms/graph';
import { GraphAlgorithmSuiteImpl } from '@/algorithms/suite';

describe('图算法套件 · 统一入口（centrality/path/community/analysis）', () => {
  it('path：dijkstra/astar/floyd/bellman 调用路径正常工作', () => {
    const g = new MemoryGraph();
    g.addNode({ id: 'A', value: 'A' });
    g.addNode({ id: 'B', value: 'B' });
    g.addNode({ id: 'C', value: 'C' });
    g.addEdge({ source: 'A', target: 'B', type: 'R', weight: 1 });
    g.addEdge({ source: 'B', target: 'C', type: 'R', weight: 2 });
    g.addEdge({ source: 'A', target: 'C', type: 'R', weight: 10 });

    const suite = new GraphAlgorithmSuiteImpl(g);
    const dj = suite.path.dijkstra('A');
    expect(dj.distances.get('C')).toBe(3);

    const aStar = suite.path.astar('A', 'C', () => 0);
    expect(aStar).not.toBeNull();
    expect(aStar!.weight).toBe(3);

    const fw = suite.path.floydWarshall();
    expect(fw.get('A')!.get('C')).toBe(3 + 0); // 通过 A->B->C = 3

    const bf = suite.path.bellmanFord('A');
    expect(bf.distances.get('C')).toBe(3);
  });

  it('analysis：桥边/关节点/环路/拓扑排序', () => {
    const g = new MemoryGraph();
    // 线性图 A->B->C，两个边均为桥边；关节点为 B
    g.addNode({ id: 'A', value: 'A' });
    g.addNode({ id: 'B', value: 'B' });
    g.addNode({ id: 'C', value: 'C' });
    g.addEdge({ source: 'A', target: 'B', type: 'R', weight: 1 });
    g.addEdge({ source: 'B', target: 'C', type: 'R', weight: 1 });

    const suite = new GraphAlgorithmSuiteImpl(g);
    const bridges = suite.analysis.findBridges();
    expect(bridges.length).toBe(2);

    const aps = suite.analysis.findArticulationPoints();
    expect(aps).toContain('B');

    const topo = suite.analysis.topologicalSort();
    expect(topo).not.toBeNull();
    expect(topo!.length).toBe(3);

    // 添加环 C->A，出现环路，拓扑排序应返回 null，detectCycles 非空
    g.addEdge({ source: 'C', target: 'A', type: 'R', weight: 1 });
    const suite2 = new GraphAlgorithmSuiteImpl(g);
    const cycles = suite2.analysis.detectCycles();
    expect(cycles.length).toBeGreaterThan(0);
    expect(suite2.analysis.topologicalSort()).toBeNull();
  });

  it('centrality/community：基础调用不抛错（小图）', () => {
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
