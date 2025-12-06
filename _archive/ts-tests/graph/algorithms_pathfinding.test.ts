import { describe, it, expect } from 'vitest';
import { GraphBuilder } from '@/extensions/algorithms/graph';
import {
  DijkstraPathAlgorithm,
  AStarPathAlgorithm,
  FloydWarshallPathAlgorithm,
  BellmanFordPathAlgorithm,
  PathAlgorithmFactory,
} from '@/extensions/algorithms/pathfinding';

describe('图算法 · 路径查找（Dijkstra/A*/Floyd/Bellman-Ford）', () => {
  it('Dijkstra：应找到加权最短路径，并返回正确权重与节点序列', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'B', 'R', 1)
      .addEdge('B', 'C', 'R', 2)
      .addEdge('A', 'C', 'R', 10)
      .build();

    const algo = new DijkstraPathAlgorithm();
    const path = algo.findShortestPath(g, 'A', 'C');
    expect(path).not.toBeNull();
    expect(path!.nodes).toEqual(['A', 'B', 'C']);
    expect(path!.length).toBe(2);
    expect(path!.weight).toBe(3);

    const all = algo.findShortestPaths(g, 'A');
    expect(all.distances.get('C')).toBe(3);
  });

  it('A*：启发式为0时等价于 Dijkstra，应返回同样的最短路径', () => {
    const g = new GraphBuilder()
      .addNode('S')
      .addNode('X')
      .addNode('T')
      .addEdge('S', 'X', 'R', 3)
      .addEdge('X', 'T', 'R', 3)
      .addEdge('S', 'T', 'R', 10)
      .build();

    const algo = new AStarPathAlgorithm();
    const path = algo.findShortestPath(g, 'S', 'T', { heuristic: () => 0 });
    expect(path).not.toBeNull();
    expect(path!.nodes).toEqual(['S', 'X', 'T']);
    expect(path!.weight).toBe(6);
  });

  it('Floyd-Warshall：应返回所有点对的最短距离', () => {
    const g = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'B', 'R', 2)
      .addEdge('B', 'C', 'R', 2)
      .addEdge('A', 'C', 'R', 5)
      .build();

    const algo = new FloydWarshallPathAlgorithm();
    const allPairs = algo.findAllShortestPaths(g);
    expect(allPairs.get('A')!.get('C')).toBe(4);
    const fromA = algo.findShortestPaths(g, 'A');
    expect(fromA.distances.get('C')).toBe(4);
  });

  it('Bellman-Ford：支持负权边，检测负环并抛错', () => {
    // 无负环：A->B(1), B->C(-2), A->C(1) => A->C 应为 -1（A->B->C）
    const g1 = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'B', 'R', 1)
      .addEdge('B', 'C', 'R', -2)
      .addEdge('A', 'C', 'R', 1)
      .build();

    const bf = new BellmanFordPathAlgorithm();
    const res = bf.findShortestPaths(g1, 'A');
    expect(res.distances.get('C')).toBe(-1);
    expect(res.paths.get('C')!.nodes).toEqual(['A', 'B', 'C']);

    // 有负环：在上图基础上添加 C->A(-5)
    const g2 = new GraphBuilder()
      .addNode('A')
      .addNode('B')
      .addNode('C')
      .addEdge('A', 'B', 'R', 1)
      .addEdge('B', 'C', 'R', -2)
      .addEdge('C', 'A', 'R', -5)
      .build();

    expect(() => bf.findShortestPaths(g2, 'A')).toThrow(/负权重回路/);
  });

  it('工厂：按条件选择算法类型', () => {
    const g = new GraphBuilder().addNode('N1').addNode('N2').addEdge('N1', 'N2', 'R', 1).build();
    expect(() => PathAlgorithmFactory.create('dijkstra')).not.toThrow();
    expect(() => PathAlgorithmFactory.create('astar')).not.toThrow();
    expect(() => PathAlgorithmFactory.create('floyd')).not.toThrow();
    expect(() => PathAlgorithmFactory.create('bellman')).not.toThrow();
  });
});
