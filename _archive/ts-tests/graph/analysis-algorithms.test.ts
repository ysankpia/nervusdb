import { describe, it, expect } from 'vitest';
import { MemoryGraph } from '@/extensions/algorithms/graph';
import { GraphAlgorithmSuiteImpl } from '@/extensions/algorithms/suite';

describe('图算法套件 · Analysis Algorithms', () => {
  it('桥边/关节点/环路/拓扑排序', () => {
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
});
