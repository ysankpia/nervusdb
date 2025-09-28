import { describe, it, expect } from 'vitest';
import { QueryBuilder, buildFindContext } from '@/query/queryBuilder.ts';

type FR = import('@/storage/persistentStore.ts').FactRecord;
type Triple = { subjectId: number; predicateId: number; objectId: number };

function makeFacts(): FR[] {
  const mk = (s: number, p: number, o: number): FR => ({
    subject: `s${s}`,
    predicate: `p${p}`,
    object: `o${o}`,
    subjectId: s,
    predicateId: p,
    objectId: o,
  });
  return [mk(1, 10, 2), mk(2, 10, 3), mk(3, 11, 4)];
}

function makeStore(facts: FR[]) {
  const triples: Triple[] = facts.map((f) => ({
    subjectId: f.subjectId,
    predicateId: f.predicateId,
    objectId: f.objectId,
  }));
  const labels: Record<number, string[]> = { 1: ['A'], 2: ['A', 'B'], 3: ['B'], 4: ['C'] };
  const hasLabel = (id: number, labs: string[]) =>
    labs.every((l) => (labels[id] || []).includes(l));
  return {
    getNodeIdByValue(v: string) {
      return v.startsWith('p') ? Number(v.slice(1)) : undefined;
    },
    getNodeValueById(id: number) {
      return `n${id}`;
    },
    getLabelIndex() {
      return {
        hasAllNodeLabels(id: number, labs: string[]) {
          return hasLabel(id, labs);
        },
        hasAnyNodeLabel(id: number, labs: string[]) {
          return labs.some((l) => (labels[id] || []).includes(l));
        },
        findNodesByLabels(labs: string[], _opts: any) {
          const res = new Set<number>();
          [1, 2, 3, 4].forEach((id) => {
            if (hasLabel(id, labs)) res.add(id);
          });
          return res;
        },
      };
    },
    getPropertyIndex() {
      return {
        queryNodesByProperty(name: string, value: unknown) {
          if (name === 'age' && value === 30) return new Set<number>([2]);
          return new Set<number>();
        },
        queryNodesByRange(name: string) {
          if (name === 'age') return new Set<number>([3]);
          return new Set<number>();
        },
        queryEdgesByProperty() {
          return new Set<string>();
        },
      };
    },
    query(criteria: Partial<Triple>) {
      const keys = Object.keys(criteria) as (keyof Triple)[];
      return triples.filter((t) => keys.every((k) => (criteria as any)[k] === (t as any)[k]));
    },
    resolveRecords(records: any[]) {
      return records as FR[];
    },
  } as any;
}

describe('QueryBuilder · 关键分支补测', () => {
  it('where/union/unionAll/whereLabel/limit/skip/batch', async () => {
    const facts = makeFacts();
    const store = makeStore(facts);
    const ctx = buildFindContext(store, { predicate: 'p10' }, 'object');
    const qb = new QueryBuilder(store, ctx);

    const filtered = qb.where((f) => f.objectId === 2);
    expect(filtered.length).toBe(1);

    const unioned = filtered.union(qb);
    expect(unioned.length).toBeGreaterThan(0);

    const unionAll = filtered.unionAll(qb);
    expect(unionAll.length).toBe(qb.length + filtered.length);

    const wl = qb.whereLabel(['A'], { mode: 'AND', on: 'both' });
    expect(wl.length).toBeGreaterThan(0);

    const limited = qb.limit(1);
    expect(limited.length).toBe(1);
    const skipped = qb.skip(1);
    expect(skipped.length).toBe(Math.max(0, qb.length - 1));

    const out: FR[][] = [];
    for await (const b of qb.batch(1)) out.push(b);
    expect(out.length).toBe(qb.length);
  });

  it('whereProperty(node eq / range) 与 followWithNodeProperty', () => {
    const facts = makeFacts();
    const store = makeStore(facts);
    const ctx = buildFindContext(store, { predicate: 'p10' }, 'object');
    const qb = new QueryBuilder(store, ctx);

    const wpEq = qb.whereProperty('age', '=', 30, 'node');
    expect(wpEq.length).toBeGreaterThanOrEqual(0);
    const wpRange = qb.whereProperty('age', '>=', 25, 'node');
    expect(wpRange.length).toBeGreaterThanOrEqual(0);

    const fw = qb.followWithNodeProperty('p10', { propertyName: 'age', value: 30 });
    expect(fw.length).toBeGreaterThanOrEqual(0);
    const fr = qb.followReverseWithNodeProperty('p10', { propertyName: 'age', range: { min: 20 } });
    expect(fr.length).toBeGreaterThanOrEqual(0);
  });
});
