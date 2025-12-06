import { describe, it, expect } from 'vitest';
import { AggregationPipeline } from '@/extensions/query/aggregation';

// 通过 from() 方式直接喂入最小数据，避免依赖持久层
type R = {
  subject: string;
  predicate: string;
  object: string;
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
};

const rows: R[] = [
  { subject: 'a', predicate: 'P', object: 'x', subjectProperties: { age: 10 } },
  { subject: 'b', predicate: 'P', object: 'x', subjectProperties: { age: 20 } },
  { subject: 'c', predicate: 'P', object: 'y', subjectProperties: { age: 30 } },
];

// 构造一个极简的 Mock store，仅满足 AggregationPipeline.from 路径（不使用 match/matchStream）
const mockStore: any = {};

describe('AggregationPipeline · 基础聚合', () => {
  it('按 object 分组做 count/sum/avg，并排序限制', () => {
    const pipeline = new AggregationPipeline(mockStore)
      .from(rows as any)
      .groupBy(['object'])
      .count('c')
      .sum('subjectProperties.age', 's')
      .avg('subjectProperties.age', 'a')
      .orderBy('c', 'DESC')
      .limit(1);

    const out = pipeline.execute();
    expect(out.length).toBe(1);
    // object=x 有两条
    expect(out[0]['object']).toBe('x');
    expect(out[0]['c']).toBe(2);
    expect(out[0]['s']).toBe(30);
    expect(out[0]['a']).toBe(15);
  });
});
