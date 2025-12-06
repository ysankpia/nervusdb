import { describe, it, expect } from 'vitest';
import { MemoryGraph } from '@/extensions/algorithms/graph';
import { GraphAlgorithmSuiteImpl } from '@/extensions/algorithms/suite';

describe('图算法套件 · Path Algorithms', () => {
  it('dijkstra/astar/floyd/bellman 调用路径正常工作', () => {
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
});
