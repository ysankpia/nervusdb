import { describe, it, expect } from 'vitest';
import { GraphTraversalSource } from '@/query/gremlin/source.ts';

// 伪造最小 PersistentStore（不触发任何 IO）
const store = {} as unknown as import('@/storage/persistentStore').PersistentStore;

describe('Gremlin GraphTraversalSource · 纯配置路径', () => {
  it('withStrategies/withoutStrategies/withSideEffect/withPath/withBulk/clone/getStats', () => {
    const g = new GraphTraversalSource(store);
    const g1 = g.withStrategies({ name: 'S1', configuration: {} } as any);
    expect(g1).toBeInstanceOf(GraphTraversalSource);
    expect(g1.getStrategies().length).toBe(1);

    const g2 = g1.withoutStrategies('S1');
    expect(g2.getStrategies().length).toBe(0);

    const g3 = g2.withSideEffect('k', 1).withPath().withBulk(true);
    const stats = g3.getStats();
    expect(stats.totalTraversals).toBe(0);

    const g4 = g3.clone();
    expect(g4).toBeInstanceOf(GraphTraversalSource);

    // 覆盖 clearCache
    g4.clearCache();
    expect(g4.getStats().cacheHitRate).toBe(0);
  });
});
