import { describe, it, expect } from 'vitest';

import { MemoryGraph } from '@/extensions/algorithms/graph.ts';

describe('MemoryGraph · 基础操作', () => {
  it('节点与有向边的新增/查询/删除', () => {
    const g = new MemoryGraph();
    g.addNode({ id: 'A', value: 'A' });
    g.addNode({ id: 'B', value: 'B' });
    g.addEdge({ source: 'A', target: 'B', type: 'EDGE', directed: true });

    expect(g.hasNode('A')).toBe(true);
    expect(g.hasNode('B')).toBe(true);
    expect(g.hasEdge('A', 'B')).toBe(true);

    expect(g.getOutDegree('A')).toBe(1);
    expect(g.getInDegree('B')).toBe(1);
    expect(g.getDegree('A')).toBe(1);
    expect(g.getDegree('B')).toBe(1);

    // 邻接查询
    expect(g.getNeighbors('A').map((n) => n.id)).toEqual(['B']);
    expect(g.getOutEdges('A')).toHaveLength(1);
    expect(g.getInEdges('B')).toHaveLength(1);

    // 删除边
    g.removeEdge('A', 'B');
    expect(g.hasEdge('A', 'B')).toBe(false);
    expect(g.getOutDegree('A')).toBe(0);
    expect(g.getInDegree('B')).toBe(0);

    // 删除节点
    g.removeNode('A');
    expect(g.hasNode('A')).toBe(false);
  });

  it('无向边应产生双向邻接并在删除时对称清理', () => {
    const g = new MemoryGraph();
    g.addNode({ id: 'X', value: 'X' });
    g.addNode({ id: 'Y', value: 'Y' });
    g.addEdge({ source: 'X', target: 'Y', type: 'EDGE', directed: false });

    // 双向度数
    expect(g.hasEdge('X', 'Y')).toBe(true);
    expect(g.hasEdge('Y', 'X')).toBe(true);
    expect(g.getOutDegree('X')).toBe(1);
    expect(g.getOutDegree('Y')).toBe(1);

    // 删除单向调用应清理对应反向边
    g.removeEdge('X', 'Y');
    expect(g.hasEdge('X', 'Y')).toBe(false);
    expect(g.hasEdge('Y', 'X')).toBe(false);
    expect(g.getOutDegree('X')).toBe(0);
    expect(g.getOutDegree('Y')).toBe(0);
  });
});
