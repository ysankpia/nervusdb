import { afterEach, describe, expect, it } from 'vitest';

import { __setNativeCoreForTesting } from '../../../src/native/core.js';
import { NervusDB } from '../../../src/nervusDb.js';

describe('NervusDB cypherQuery', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
  });

  it('delegates to native executeQuery (never to triple query)', async () => {
    const calls: Array<{ type: string; payload?: unknown }> = [];

    __setNativeCoreForTesting({
      open: () =>
        ({
          executeQuery(query: string, params?: Record<string, unknown> | null) {
            calls.push({ type: 'executeQuery', payload: { query, params } });
            return [{ ok: true }];
          },
          query() {
            calls.push({ type: 'query' });
            return [];
          },
          close() {
            calls.push({ type: 'close' });
          },
        }) as any,
    });

    const db = await NervusDB.open('tmp-cypher.redb', { experimental: { cypher: true } });
    const res = await db.cypherQuery('MATCH (n) RETURN n', { limit: 1 });
    await db.close();

    expect(res.records).toEqual([{ ok: true }]);
    expect(calls.filter((c) => c.type === 'executeQuery')).toHaveLength(1);
    expect(calls.some((c) => c.type === 'query')).toBe(false);
  });

  it('fails fast when cypher is disabled', async () => {
    __setNativeCoreForTesting({
      open: () =>
        ({
          executeQuery() {
            return [];
          },
          close() {},
        }) as any,
    });

    const db = await NervusDB.open('tmp-cypher-disabled.redb', { experimental: { cypher: false } });
    await expect(db.cypherQuery('MATCH (n) RETURN n')).rejects.toThrow(/Cypher/);
    await db.close();
  });
});
