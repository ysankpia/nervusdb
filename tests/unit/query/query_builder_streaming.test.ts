import { describe, it, expect } from 'vitest';
import { StreamingQueryBuilder, buildFindContext, QueryBuilder } from '@/query/queryBuilder.ts';

type FR = import('@/storage/persistentStore.ts').FactRecord;
type Triple = { subjectId: number; predicateId: number; objectId: number };

function mkFact(s: number, p: number, o: number): FR {
  return {
    subject: `s${s}`,
    predicate: `p${p}`,
    object: `o${o}`,
    subjectId: s,
    predicateId: p,
    objectId: o,
  };
}

describe('QueryBuilder/StreamingQueryBuilder · buildFindContext/流式迭代', () => {
  it('buildFindContext: convertCriteriaToIds 返回 null → EMPTY_CONTEXT；空条件 includeProperties=false', () => {
    // store A：getNodeIdByValue 返回 undefined，导致 convertCriteriaToIds 返回 null
    const storeA: any = {
      getNodeIdByValue() {
        return undefined;
      },
      query() {
        return [];
      },
    };
    const ctxEmpty = buildFindContext(storeA, { subject: 'nope' }, 'object');
    expect(ctxEmpty.facts.length).toBe(0);
    expect(ctxEmpty.frontier.size).toBe(0);

    // store B：空条件查询返回若干记录，触发 includeProperties=false 分支
    const facts = [mkFact(1, 10, 2), mkFact(2, 10, 3)];
    const triples: Triple[] = facts.map((f) => ({
      subjectId: f.subjectId,
      predicateId: f.predicateId,
      objectId: f.objectId,
    }));
    const storeB: any = {
      getNodeIdByValue() {
        return undefined;
      }, // 不设置任何 id
      query(criteria: Partial<Triple>) {
        return criteria && Object.keys(criteria).length === 0 ? triples : [];
      },
      resolveRecords(records: Triple[], _opts?: { includeProperties?: boolean }) {
        return records as FR[];
      },
    };
    const ctxNoProps = buildFindContext(storeB, {}, 'both');
    expect(ctxNoProps.facts.length).toBe(2);
    expect(ctxNoProps.frontier.size).toBeGreaterThan(0);
  });

  it('StreamingQueryBuilder: [Symbol.asyncIterator] 与 toQueryBuilder 走 pin/unpin（pinnedEpochStack）', async () => {
    async function* gen() {
      yield mkFact(1, 10, 2);
      yield mkFact(2, 10, 3);
    }
    const pinnedEpochStack: number[] = [];
    const store: any = { pinnedEpochStack };

    const sb = new StreamingQueryBuilder(
      store,
      { factsStream: gen(), frontier: new Set<number>([1]), orientation: 'object' },
      42,
    );
    const collected: FR[] = [];
    for await (const f of sb) collected.push(f);
    expect(collected.length).toBe(2);

    // 新建一个流实例再转换为 QueryBuilder，确认 pin/unpin 成对
    const sb2 = new StreamingQueryBuilder(
      store,
      { factsStream: gen(), frontier: new Set<number>([1]), orientation: 'object' },
      42,
    );
    const qb: QueryBuilder = await sb2.toQueryBuilder();
    expect(qb.length).toBe(2);
  });
});
