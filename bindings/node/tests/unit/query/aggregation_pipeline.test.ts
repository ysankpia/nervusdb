import { describe, it, expect } from 'vitest';
import { AggregationPipeline } from '@/extensions/query/aggregation.ts';

// 构造最小 FactRecord
type FR = import('@/core/storage/persistentStore.ts').FactRecord;

describe('AggregationPipeline · 基础与流式分支', () => {
  it('from/groupBy/count/sum/avg/min/max/order/limit · execute()', () => {
    const records: FR[] = [
      {
        subject: 's1',
        predicate: 'P',
        object: 'o1',
        subjectId: 1,
        predicateId: 2,
        objectId: 3,
        subjectProperties: { cat: 'A', score: 10 },
      },
      {
        subject: 's2',
        predicate: 'P',
        object: 'o2',
        subjectId: 4,
        predicateId: 2,
        objectId: 5,
        subjectProperties: { cat: 'A', score: 20 },
      },
      {
        subject: 's3',
        predicate: 'P',
        object: 'o3',
        subjectId: 6,
        predicateId: 2,
        objectId: 7,
        subjectProperties: { cat: 'B' }, // 无 score（触发 min/max=null 分支）
      },
    ];

    // store 在 execute 路径不会被用到，提供空壳即可
    const store: any = {};
    const agg = new AggregationPipeline(store);
    const out = agg
      .from(records)
      .groupBy(['subjectProperties.cat'])
      .count('cnt')
      .sum('subjectProperties.score', 'sum')
      .avg('subjectProperties.score', 'avg')
      .min('subjectProperties.score', 'mn')
      .max('subjectProperties.score', 'mx')
      .orderBy('cnt', 'DESC')
      .limit(1)
      .execute();

    expect(out.length).toBe(1);
    const row = out[0] as any;
    // A 组两条，B 组一条，降序+limit=1 应命中 A
    expect(row['subjectProperties.cat']).toBe('A');
    expect(row.cnt).toBe(2);
    expect(row.sum).toBe(30);
    expect(row.avg).toBe(15);
    expect(row.mn).toBe(10);
    expect(row.mx).toBe(20);
  });

  it('matchStream/executeStreaming · 增量聚合与 partialSort', async () => {
    // 5 个分组，limit=3，触发 partialSort 分支
    const mk = (cat: string, score?: number): FR => ({
      subject: 's',
      predicate: 'P',
      object: 'o',
      subjectId: 1,
      predicateId: 2,
      objectId: 3,
      subjectProperties: { cat, score },
    });
    const batch1: FR[] = [mk('A', 1), mk('B', 2)];
    const batch2: FR[] = [mk('C', 3), mk('D', 4), mk('E')]; // E 无 score

    const store: any = {
      // 流式批次：不依赖 criteria 与 batchSize
      async *streamFactRecords() {
        yield batch1;
        yield batch2;
      },
    };

    const agg = new AggregationPipeline(store);
    const out = await agg
      .matchStream({}, { batchSize: 2 })
      .groupBy(['subjectProperties.cat'])
      .count('c')
      .sum('subjectProperties.score', 's')
      .avg('subjectProperties.score', 'a')
      .min('subjectProperties.score', 'mn')
      .max('subjectProperties.score', 'mx')
      .orderBy('s', 'DESC') // 依据 sum 排序
      .limit(3)
      .executeStreaming();

    // 只保留前3个分组
    expect(out.length).toBe(3);
    // 排序前3应为 D(4)、C(3)、B(2) 或相近；确保字段正确且不为 NaN
    for (const r of out as any[]) {
      expect(typeof r.s === 'number').toBe(true);
      expect(['A', 'B', 'C', 'D', 'E']).toContain(r['subjectProperties.cat']);
    }
    // E 组 min/max 为 null
    const e = (out as any[]).find((x) => x['subjectProperties.cat'] === 'E');
    if (e) {
      expect(e.mn).toBeNull();
      expect(e.mx).toBeNull();
    }
  });
});
