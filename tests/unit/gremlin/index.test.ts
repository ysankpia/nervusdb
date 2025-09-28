import { describe, it, expect } from 'vitest';

// 只验证导出与谓词工厂，不深入执行器（保持单测轻量）
import { P, gremlin, createGremlinSource, GraphTraversalSource } from '@/query/gremlin/index.ts';

// 以最小代价构造一个假的 PersistentStore（仅用于类型占位，避免真实 IO）
// GremlinTraversalSource 构造函数不会立刻访问 store，仅持有引用
const fakeStore = {} as unknown as import('@/storage/persistentStore.ts').PersistentStore;

describe('Gremlin 入口与谓词工厂', () => {
  it('P 谓词工厂应返回结构化对象', () => {
    expect(P.eq(1)).toEqual({ operator: 'eq', value: 1 });
    expect(P.neq('x')).toEqual({ operator: 'neq', value: 'x' });
    expect(P.lt(3)).toEqual({ operator: 'lt', value: 3 });
    expect(P.lte(3)).toEqual({ operator: 'lte', value: 3 });
    expect(P.gt(2)).toEqual({ operator: 'gt', value: 2 });
    expect(P.gte(2)).toEqual({ operator: 'gte', value: 2 });
    expect(P.inside(1, 5)).toEqual({ operator: 'inside', value: 1, other: 5 });
    expect(P.outside(1, 5)).toEqual({ operator: 'outside', value: 1, other: 5 });
    expect(P.between(1, 5)).toEqual({ operator: 'between', value: 1, other: 5 });
    expect(P.within(1, 2, 3)).toEqual({ operator: 'within', value: [1, 2, 3] });
    expect(P.without('a', 'b')).toEqual({ operator: 'without', value: ['a', 'b'] });
    expect(P.startingWith('pre')).toEqual({ operator: 'startingWith', value: 'pre' });
    expect(P.endingWith('suf')).toEqual({ operator: 'endingWith', value: 'suf' });
    expect(P.containing('mid')).toEqual({ operator: 'containing', value: 'mid' });
    expect(P.notStartingWith('pre')).toEqual({ operator: 'notStartingWith', value: 'pre' });
    expect(P.notEndingWith('suf')).toEqual({ operator: 'notEndingWith', value: 'suf' });
    expect(P.notContaining('mid')).toEqual({ operator: 'notContaining', value: 'mid' });
  });

  it('gremlin()/createGremlinSource 应该返回 GraphTraversalSource 实例', () => {
    const g1 = gremlin(fakeStore);
    const g2 = createGremlinSource(fakeStore, { batchSize: 10, enableOptimization: true });
    expect(g1).toBeInstanceOf(GraphTraversalSource);
    expect(g2).toBeInstanceOf(GraphTraversalSource);
  });
});
