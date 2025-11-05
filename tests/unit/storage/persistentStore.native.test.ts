import { describe, expect, it, afterEach } from 'vitest';

import { __setNativeCoreForTesting } from '../../../src/native/core.js';
import { PersistentStore } from '../../../src/core/storage/persistentStore.js';

describe('PersistentStore native bridge', () => {
  afterEach(async () => {
    __setNativeCoreForTesting(undefined);
  });

  it('invokes native binding when available', async () => {
    const calls: Array<{ type: string; payload?: unknown }> = [];

    const mockHandle = {
      hydrate(dictionary: string[], triples: Array<Record<string, number>>) {
        calls.push({ type: 'hydrate', payload: { dictionary, triples } });
      },
      addFact(subject: string, predicate: string, object: string) {
        calls.push({ type: 'add', payload: { subject, predicate, object } });
        return { subject_id: 1, predicate_id: 2, object_id: 3 };
      },
      query(criteria?: Record<string, unknown>) {
        calls.push({ type: 'query', payload: criteria ?? {} });
        return [
          { subject_id: 1, predicate_id: 2, object_id: 3 },
          { subject_id: 4, predicate_id: 5, object_id: 6 },
        ];
      },
      close() {
        calls.push({ type: 'close' });
      },
    };

    __setNativeCoreForTesting({
      open: ({ dataPath }) => {
        calls.push({ type: 'open', payload: dataPath });
        return mockHandle;
      },
    });

    const store = await PersistentStore.open(':memory:');
    store.addFact({ subject: 'alice', predicate: 'knows', object: 'bob' });
    const triples = store.query({});
    expect(triples).toEqual([
      { subjectId: 1, predicateId: 2, objectId: 3 },
      { subjectId: 4, predicateId: 5, objectId: 6 },
    ]);

    const streamed: Array<{ subjectId: number; predicateId: number; objectId: number }> = [];
    for await (const triple of store.queryStreaming({ objectId: 6 })) {
      streamed.push(triple);
    }

    expect(streamed).toEqual([
      { subjectId: 1, predicateId: 2, objectId: 3 },
      { subjectId: 4, predicateId: 5, objectId: 6 },
    ]);
    await store.close();

    expect(calls.find((c) => c.type === 'hydrate')).toMatchObject({
      payload: { dictionary: [], triples: [] },
    });
    expect(calls.find((c) => c.type === 'open')).toBeTruthy();
    expect(calls.find((c) => c.type === 'add')).toMatchObject({
      payload: { subject: 'alice', predicate: 'knows', object: 'bob' },
    });
    const queryCalls = calls.filter((c) => c.type === 'query');
    expect(queryCalls).toHaveLength(2);
    expect(queryCalls[0]?.payload).toEqual({});
    expect(queryCalls[1]?.payload).toEqual({ object_id: 6 });
    expect(calls.some((c) => c.type === 'close')).toBe(true);
  });
});
