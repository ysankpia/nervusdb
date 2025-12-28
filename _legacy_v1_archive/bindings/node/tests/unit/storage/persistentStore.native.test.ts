import { describe, expect, it, afterEach } from 'vitest';

import { __setNativeCoreForTesting } from '../../../src/native/core.js';
import { PersistentStore } from '../../../src/core/storage/persistentStore.js';

describe('PersistentStore native bridge', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
  });

  it('delegates add/query/stream to native handle', async () => {
    const calls: Array<{ type: string; payload?: unknown }> = [];

    const strToId = new Map<string, number>([
      ['alice', 1],
      ['knows', 2],
      ['bob', 3],
      ['carol', 4],
    ]);
    const idToStr = new Map<number, string>([
      [1, 'alice'],
      [2, 'knows'],
      [3, 'bob'],
      [4, 'carol'],
    ]);

    const mockHandle = {
      addFact(subject: string, predicate: string, object: string) {
        calls.push({ type: 'add', payload: { subject, predicate, object } });
        return {
          subjectId: strToId.get(subject) ?? 0,
          predicateId: strToId.get(predicate) ?? 0,
          objectId: strToId.get(object) ?? 0,
        };
      },
      deleteFact() {
        calls.push({ type: 'delete' });
        return true;
      },
      resolveId(value: string) {
        calls.push({ type: 'resolve_id', payload: value });
        return strToId.get(value) ?? null;
      },
      resolveStr(id: number) {
        calls.push({ type: 'resolve_str', payload: id });
        return idToStr.get(id) ?? null;
      },
      query(criteria?: Record<string, unknown>) {
        calls.push({ type: 'query', payload: criteria ?? {} });
        return [
          { subjectId: 1, predicateId: 2, objectId: 3 },
          { subjectId: 1, predicateId: 2, objectId: 4 },
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
            triples: [{ subjectId: 1, predicateId: 2, objectId: 3 }],
            done: false,
          };
        }
        return {
          triples: [{ subjectId: 1, predicateId: 2, objectId: 4 }],
          done: true,
        };
      },
      closeCursor(cursorId: number) {
        calls.push({ type: 'cursor_close', payload: { cursorId } });
      },
      intern(value: string) {
        calls.push({ type: 'intern', payload: value });
        const existing = strToId.get(value);
        if (existing) return existing;
        const next = strToId.size + 1;
        strToId.set(value, next);
        idToStr.set(next, value);
        return next;
      },
      getDictionarySize() {
        calls.push({ type: 'dict_size' });
        return strToId.size;
      },
      executeQuery(query: string) {
        calls.push({ type: 'execute_query', payload: query });
        return [];
      },
      hydrate() {
        calls.push({ type: 'hydrate' });
      },
      setNodeProperty() {},
      getNodeProperty() {
        return null;
      },
      setEdgeProperty() {},
      getEdgeProperty() {
        return null;
      },
      beginTransaction() {
        calls.push({ type: 'begin_tx' });
      },
      commitTransaction() {
        calls.push({ type: 'commit_tx' });
      },
      abortTransaction() {
        calls.push({ type: 'abort_tx' });
      },
      close() {
        calls.push({ type: 'close' });
      },
    };

    __setNativeCoreForTesting({
      open: ({ dataPath }) => {
        calls.push({ type: 'open', payload: dataPath });
        return mockHandle as any;
      },
    });

    const store = await PersistentStore.open('tmp-persistentStore-native.redb');

    store.addFact({ subject: 'alice', predicate: 'knows', object: 'bob' });
    const facts = store.query({ predicate: 'knows' });
    expect(facts).toEqual([
      {
        subject: 'alice',
        predicate: 'knows',
        object: 'bob',
        subjectId: 1,
        predicateId: 2,
        objectId: 3,
      },
      {
        subject: 'alice',
        predicate: 'knows',
        object: 'carol',
        subjectId: 1,
        predicateId: 2,
        objectId: 4,
      },
    ]);

    const streamed: Array<{ subject: string; predicate: string; object: string }> = [];
    for await (const batch of store.streamQuery({ predicate: 'knows' }, 1)) {
      streamed.push(
        ...batch.map((t) => ({ subject: t.subject, predicate: t.predicate, object: t.object })),
      );
    }
    expect(streamed).toEqual([
      { subject: 'alice', predicate: 'knows', object: 'bob' },
      { subject: 'alice', predicate: 'knows', object: 'carol' },
    ]);

    await store.close();

    expect(calls.some((c) => c.type === 'open')).toBe(true);
    expect(calls.some((c) => c.type === 'add')).toBe(true);
    expect(calls.some((c) => c.type === 'query')).toBe(true);
    expect(calls.some((c) => c.type === 'cursor_open')).toBe(true);
    expect(calls.some((c) => c.type === 'cursor_close')).toBe(true);
    expect(calls.some((c) => c.type === 'close')).toBe(true);
  });
});

