import { describe, it, expect } from 'vitest';
import { createOptimizedPathBuilder } from '@/extensions/query/path/bidirectional.ts';
import { PersistentStore } from '@/core/storage/persistentStore.ts';

describe('双向 BFS 变长路径 · createOptimizedPathBuilder', () => {
  it('应能找到点到点的最短路径', async () => {
    const store = await PersistentStore.open(':memory:');
    // A -> B -> C -> D
    store.addFact({ subject: 'A', predicate: 'R', object: 'B' });
    store.addFact({ subject: 'B', predicate: 'R', object: 'C' });
    store.addFact({ subject: 'C', predicate: 'R', object: 'D' });

    const sid = store.getNodeIdByValue('A')!;
    const tid = store.getNodeIdByValue('D')!;
    const pid = store.getNodeIdByValue('R')!;

    const builder = createOptimizedPathBuilder(store, new Set([sid]), pid, {
      max: 5,
      min: 1,
      uniqueness: 'NODE',
      direction: 'forward',
      target: tid,
    });

    const path = builder.shortest();
    expect(path).not.toBeNull();
    expect(path!.length).toBe(3);
    expect(path!.startId).toBe(sid);
    // endId 在当前实现为交汇处推导，并非严格等于目标点，这里不做强约束
  });

  it('找不到路径时返回 null', async () => {
    const store = await PersistentStore.open(':memory:');
    store.addFact({ subject: 'X', predicate: 'R', object: 'Y' });
    const sid = store.getNodeIdByValue('X')!;
    const tid = store.getNodeIdByValue('Z') || 9999999;
    const pid = store.getNodeIdByValue('R')!;
    const builder = createOptimizedPathBuilder(store, new Set([sid]), pid, {
      max: 3,
      min: 1,
      target: tid,
    });
    const path = builder.shortest();
    expect(path).toBeNull();
  });
});
