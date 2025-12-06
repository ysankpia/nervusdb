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
      openCursor(criteria?: Record<string, unknown>) {
        calls.push({ type: 'cursor_open', payload: criteria ?? {} });
        return { id: 42 };
      },
      readCursor(cursorId: number, batchSize: number) {
        calls.push({ type: 'cursor_next', payload: { cursorId, batchSize } });
        if (calls.filter((c) => c.type === 'cursor_next').length === 1) {
          return {
            triples: [{ subject_id: 1, predicate_id: 2, object_id: 3 }],
            done: false,
          };
        }
        return {
          triples: [{ subject_id: 4, predicate_id: 5, object_id: 6 }],
          done: true,
        };
      },
      closeCursor(cursorId: number) {
        calls.push({ type: 'cursor_close', payload: { cursorId } });
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
    expect(queryCalls).toHaveLength(1);
    expect(queryCalls[0]?.payload).toEqual({});
    const cursorOpens = calls.filter((c) => c.type === 'cursor_open');
    expect(cursorOpens).toHaveLength(1);
    expect(cursorOpens[0]?.payload).toEqual({ object_id: 6 });
    const cursorReads = calls.filter((c) => c.type === 'cursor_next');
    expect(cursorReads).toHaveLength(2);
    expect(cursorReads[0]?.payload).toEqual({ cursorId: 42, batchSize: 1000 });
    expect(cursorReads[1]?.payload).toEqual({ cursorId: 42, batchSize: 1000 });
    expect(calls.some((c) => c.type === 'cursor_close')).toBe(true);
    expect(calls.some((c) => c.type === 'close')).toBe(true);
  });

  it('falls back to TypeScript streaming when native cursor is missing', async () => {
    const calls: Array<{ type: string; payload?: unknown }> = [];

    const mockHandle = {
      hydrate() {
        calls.push({ type: 'hydrate' });
      },
      addFact() {
        calls.push({ type: 'add' });
        return { subject_id: 10, predicate_id: 20, object_id: 30 };
      },
      query() {
        calls.push({ type: 'query' });
        return [{ subject_id: 10, predicate_id: 20, object_id: 30 }];
      },
      close() {
        calls.push({ type: 'close' });
      },
    } as const;

    __setNativeCoreForTesting({
      open: () => mockHandle,
    });

    const store = await PersistentStore.open(':memory:');
    store.addFact({ subject: 'neo', predicate: 'loves', object: 'trinity' });

    const streamed: Array<{ subjectId: number; predicateId: number; objectId: number }> = [];
    for await (const triple of store.queryStreaming({})) {
      streamed.push(triple);
      break; // 只验证首批即可
    }

    expect(streamed).toEqual([{ subjectId: 0, predicateId: 1, objectId: 2 }]);
    expect(calls.some((c) => c.type === 'cursor_open')).toBe(false);
    await store.close();
  });
});
