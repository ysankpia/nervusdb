import { describe, it, expect } from 'vitest';

import { MemoryGraph, GraphBuilder } from '@/extensions/algorithms/graph.ts';

describe('MemoryGraph · 指标与分析', () => {
  it('getStats 应正确计算节点/边/平均度/连通性', () => {
    const g = new MemoryGraph();
    // 组件1：A->B->C（连通）
    g.addEdge({ source: 'A', target: 'B', type: 'E', directed: true });
    g.addEdge({ source: 'B', target: 'C', type: 'E', directed: true });
    // 组件2：D 独立节点
    g.addNode({ id: 'D', value: 'D' });

    const s = g.getStats();
    expect(s.nodeCount).toBe(4);
    expect(s.edgeCount).toBe(2);
    expect(s.isConnected).toBe(false);
    expect(s.componentCount).toBe(2);
    expect(s.averageDegree).toBeGreaterThan(0);
    expect(s.density).toBeGreaterThan(0);
  });

  it('clone/clear 不应相互影响且保持深拷贝语义', () => {
    const builder = new GraphBuilder();
    const g1 = builder
      .addNode('N1', 'N1', { t: 1 })
      .addNode('N2', 'N2', { t: 2 })
      .addEdge('N1', 'N2', 'E', 1, { w: 1 })
      .build();

    // 克隆后修改原图不应影响克隆
    const g2 = g1.clone();
    expect(g2.getNodes().length).toBe(2);
    expect(g2.getEdges().length).toBe(1);

    // 清空原图
    g1.clear();
    expect(g1.getNodes()).toEqual([]);
    expect(g1.getEdges()).toEqual([]);

    // 克隆图仍然保留数据
    expect(g2.getNodes().length).toBe(2);
    expect(g2.getEdges().length).toBe(1);
  });

  it('k 跳邻居/直径/聚类系数的基本校验', () => {
    // 构建环形图：0-1-2-3-0
    const builder = new GraphBuilder();
    const g = builder.cycle(4).build();

    const k1 = (g as MemoryGraph).getKHopNeighbors('0', 1);
    // 有向环：0 -> 1 -> 2 -> 3 -> 0，因此 1 跃邻居仅包含 '1'
    expect(Array.from(k1).sort()).toEqual(['1']);

    const k2 = (g as MemoryGraph).getKHopNeighbors('0', 2);
    expect(Array.from(k2).sort()).toEqual(['1', '2']);

    const diameter = (g as MemoryGraph).getDiameter();
    // 有向环(4)最远可达距离为3
    expect(diameter).toBe(3);

    const cc = (g as MemoryGraph).getClusteringCoefficient();
    expect(cc).toBeGreaterThanOrEqual(0);
    expect(cc).toBeLessThanOrEqual(1);
  });
});
