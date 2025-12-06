import { describe, it, expect } from 'vitest';
import { GraphAlgorithmSuiteImpl } from '@/extensions/algorithms/suite.ts';
import { GraphBuilder } from '@/extensions/algorithms/graph.ts';

describe('GraphAlgorithmSuiteImpl · analysis 与 utils', () => {
  it('analysis.findBridges/Articulation/DetectCycles/TopoSort 基础路径', () => {
    // 构造包含桥边与一个小环的图：
    // 组件1（DAG）：A -> B -> C  （A-B、B-C 为桥边，B 为关节点）
    // 组件2（环）：X -> Y -> Z -> X
    const g = new GraphBuilder()
      .addEdge('A', 'B', 'R')
      .addEdge('B', 'C', 'R')
      .addEdge('X', 'Y', 'R')
      .addEdge('Y', 'Z', 'R')
      .addEdge('Z', 'X', 'R')
      .build();

    const suite = new GraphAlgorithmSuiteImpl(g);

    const bridges = suite.analysis.findBridges();
    // DAG 部分存在桥边
    expect(bridges.length).toBeGreaterThan(0);

    const aps = suite.analysis.findArticulationPoints();
    expect(aps.includes('B')).toBe(true);

    const cycles = suite.analysis.detectCycles();
    expect(cycles.length).toBeGreaterThan(0);

    // 对 DAG 子图进行拓扑排序（环会导致返回 null，但我们只对无环子图断言）
    const dag = new GraphBuilder().addEdge('P', 'Q', 'R').addEdge('Q', 'R', 'R').build();
    const suiteDag = new GraphAlgorithmSuiteImpl(dag);
    const topo = suiteDag.analysis.topologicalSort();
    expect(topo).not.toBeNull();
    expect(topo!.length).toBe(3);
  });
});
