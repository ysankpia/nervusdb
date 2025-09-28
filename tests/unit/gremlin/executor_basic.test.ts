import { describe, it, expect } from 'vitest';
import { GremlinExecutor } from '@/query/gremlin/executor.ts';

type Triple = { subjectId: number; predicateId: number; objectId: number };

function makeStore() {
  const PRED: Record<string, number> = { KNOWS: 200, name: 201 };
  const predVal: Record<number, string> = { 200: 'KNOWS', 201: 'name' };
  const nodeName: Record<number, string> = { 1: 'n1', 2: 'n2', 500: 'a', 501: 'b' };

  const triples: Triple[] = [
    { subjectId: 1, predicateId: PRED.KNOWS, objectId: 2 },
    { subjectId: 1, predicateId: PRED.name, objectId: 500 },
    { subjectId: 2, predicateId: PRED.name, objectId: 501 },
  ];

  return {
    getNodeValueById(id: number) {
      return nodeName[id] ?? `n${id}`;
    },
    getNodeIdByValue(v: string) {
      return PRED[v];
    },
    getNodeProperties(id: number) {
      return { __labels: ['L'], name: id === 1 ? 'a' : 'b' };
    },
    query(criteria: Partial<Triple>) {
      const keys = Object.keys(criteria) as (keyof Triple)[];
      return triples.filter((t) => keys.every((k) => (criteria as any)[k] === (t as any)[k]));
    },
    resolveRecords(records: Triple[]) {
      return records;
    },
  } as any;
}

describe('GremlinExecutor · 基础步骤执行', () => {
  it('V → has(name) → out(KNOWS) → hasLabel → values/name → valueMap → elementMap → count/fold', async () => {
    const store = makeStore();
    const exec = new GremlinExecutor(store);

    const steps: any[] = [
      { type: 'V' },
      { type: 'has', key: 'name' },
      { type: 'out', edgeLabels: ['KNOWS'] },
      { type: 'hasLabel', labels: ['L'] },
      { type: 'values', propertyKeys: ['name'] },
      { type: 'valueMap', propertyKeys: ['value'] },
      { type: 'elementMap', propertyKeys: ['value'] },
      { type: 'count' },
      { type: 'fold' },
    ];

    const res = await exec.execute(steps);
    expect(Array.isArray(res)).toBe(true);
    // 最终 fold 返回单元素（list），但外层包装为 TraversalResult，bulk=1
    expect(res.length).toBe(1);
    expect((res[0] as any).value.type).toBe('list');
  });

  it('outE/inV/bothE/bothV/hasId/is/range/skip/dedup/as/select', async () => {
    const store = makeStore();
    const exec = new GremlinExecutor(store);

    const steps: any[] = [
      { type: 'V' },
      { type: 'outE', edgeLabels: ['KNOWS'] },
      { type: 'inV' },
      { type: 'as', stepLabel: 'v' },
      { type: 'select', selectKeys: ['v'] },
      { type: 'values', propertyKeys: ['name'] },
      { type: 'is', predicate: { operator: 'eq', value: 'b' } },
      { type: 'bothE', edgeLabels: ['KNOWS'] },
      { type: 'bothV' },
      { type: 'range', low: 0, high: 1 },
      { type: 'skip', skip: 0 },
      { type: 'dedup' },
      { type: 'count' },
    ];

    const res = await exec.execute(steps);
    expect(res.length).toBe(1);
    expect((res[0] as any).value.type).toBe('count');
  });
});
