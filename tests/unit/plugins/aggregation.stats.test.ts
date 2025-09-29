import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { makeWorkspace, cleanupWorkspace, within } from '../../helpers/tempfs';
import { SynapseDB } from '@/synapseDb';
import { AggregationPlugin } from '@/plugins/aggregation';

describe('AggregationPlugin 统计与分布', () => {
  let ws: string;
  beforeAll(async () => {
    ws = await makeWorkspace('agg-plugin');
  });
  afterAll(async () => {
    await cleanupWorkspace(ws);
  });

  it('空图应返回全 0 与空结构', async () => {
    const db = await SynapseDB.open(within(ws, 'empty.synapsedb'), {
      pageSize: 256,
    });

    const agg = db.plugin<AggregationPlugin>('aggregation')!;
    const stats = agg.getStatsSummary();

    expect(stats.nodes).toBe(0);
    expect(stats.edges).toBe(0);
    expect(stats.predicates).toBe(0);
    expect(stats.avgDegree).toBe(0);
    expect(stats.connectedComponents).toBe(0);
    expect(stats.topPredicates).toEqual([]);

    await db.close();
  });

  it('应正确统计节点/边/谓词分布、度与连通分量', async () => {
    const db = await SynapseDB.open(within(ws, 'graph.synapsedb'), {
      pageSize: 256,
    });

    // 构造 4 个连通分量：{A,B,C}, {X,Y}, {D,Z}, {E,W}
    const facts = [
      { subject: 'A', predicate: 'knows', object: 'B' },
      { subject: 'B', predicate: 'knows', object: 'C' },
      { subject: 'X', predicate: 'likes', object: 'Y' },
      { subject: 'D', predicate: 'worksAt', object: 'Z' },
      { subject: 'E', predicate: 'worksAt', object: 'W' },
    ] as const;
    for (const f of facts) db.addFact({ ...f });

    const agg = db.plugin<AggregationPlugin>('aggregation')!;

    // 节点数量：当前实现基于字典大小（包含谓词值）
    // 唯一值：A,B,C,X,Y,D,E,Z,W + knows,likes,worksAt = 12
    expect(agg.countNodes()).toBe(12);

    // 边数量
    expect(agg.countEdges()).toBe(facts.length);

    // 按谓词计数
    expect(agg.countByPredicate()).toEqual({ knows: 2, likes: 1, worksAt: 2 });

    // 度分布
    const { inDegree, outDegree } = agg.getDegreeDistribution();
    expect(outDegree['A']).toBe(1);
    expect(inDegree['B']).toBe(1);
    expect(outDegree['B']).toBe(1);
    expect(inDegree['C']).toBe(1);
    expect(outDegree['X']).toBe(1);
    expect(inDegree['Y']).toBe(1);
    expect(outDegree['D']).toBe(1);
    expect(inDegree['Z']).toBe(1);
    expect(outDegree['E']).toBe(1);
    expect(inDegree['W']).toBe(1);

    // Top 节点（total/in/out）
    const topTotal = agg.getTopNodes(3, 'total');
    // B 有 2（入1+出1），其他均 1
    expect(topTotal[0]).toEqual({ node: 'B', degree: 2 });
    const topIn = agg.getTopNodes(2, 'in');
    expect(topIn.map((x) => x.degree)).toEqual([1, 1]);
    const topOut = agg.getTopNodes(2, 'out');
    expect(topOut.map((x) => x.degree)).toEqual([1, 1]);

    // 连通分量
    expect(agg.countConnectedComponents()).toBe(4);

    // 汇总
    const sum = agg.getStatsSummary();
    expect(sum.edges).toBe(5);
    expect(sum.predicates).toBe(3);
    // 平均度 = 2*E/N = 10/12 ≈ 0.83，插件会保留两位小数
    expect(sum.avgDegree).toBeCloseTo(0.83, 2);
    expect(sum.connectedComponents).toBe(4);
    expect(sum.topPredicates[0]).toEqual({ predicate: 'knows', count: 2 });

    await db.flush();
    await db.close();
  });
});
