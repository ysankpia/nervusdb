import { describe, it, expect } from 'vitest';
import { GraphTraversal } from '@/query/gremlin/traversal.ts';
import { P } from '@/query/gremlin/types.ts';

describe('Gremlin Traversal · 构建器分支覆盖（不执行）', () => {
  const store: any = {}; // 不触发执行器，仅测试构建

  it('has()/is()/where() 重载 + 选择/投影/范围类步骤', () => {
    const t0 = new GraphTraversal<any, any>(store);

    const pred = { operator: P.gt, value: 10 } as const;

    const t = t0
      .V('v1')
      .out('KNOWS')
      .has('age')
      .has('name', 'alice')
      .has('age', pred)
      .has('person', 'age', 20)
      .has('person', 'age', pred)
      .hasLabel('person', 'user')
      .hasId(1, '2')
      .is('ok')
      .is({ operator: P.neq, value: 'no' })
      .where({ operator: P.gte, value: 0 })
      .where(new GraphTraversal<any, any>(store).has('flag', true))
      .order()
      .limit(5)
      .range(0, 3)
      .skip(1)
      .dedup('name')
      .as('a')
      .select('a')
      .values('name')
      .valueMap('name')
      .elementMap('name')
      .count()
      .fold();

    const steps = t.getSteps();
    // 至少包含上述链式步骤
    expect(steps.length).toBeGreaterThan(10);

    // 抽查几个关键步骤形态
    const hasStep = steps.find((s: any) => s.type === 'has' && s.key === 'age');
    expect(hasStep).toBeDefined();
    const whereStep = steps.find((s: any) => s.type === 'where');
    expect(whereStep).toBeDefined();
    const orderStep = steps.find((s: any) => s.type === 'order');
    expect(orderStep).toBeDefined();
    const dedupStep = steps.find((s: any) => s.type === 'dedup');
    expect(dedupStep).toBeDefined();
  });
});
