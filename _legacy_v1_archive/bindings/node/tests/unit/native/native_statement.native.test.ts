import { afterEach, describe, expect, it } from 'vitest';

import { __setNativeCoreForTesting } from '../../../src/native/core.js';
import { CypherValueType, NervusDB } from '../../../src/nervusDb.js';

const describeNative =
  process.env.NERVUSDB_EXPECT_NATIVE === '1' ? describe : describe.skip;

describeNative('native statement API', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;
  });

  it('iterates rows via prepareV2/step/column_*', async () => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;

    const dbPath = `tmp-native-stmt-${process.pid}-${Date.now()}`;
    const db = await NervusDB.open(dbPath);
    db.addFact({ subject: 'alice', predicate: 'knows', object: 'bob' });

    const stmt = db.cypherPrepare('MATCH (a)-[r]->(b) RETURN a, r, b');
    expect(stmt.columns).toEqual(['a', 'r', 'b']);

    let rows = 0;
    while (stmt.step()) {
      rows += 1;

      expect(stmt.columnType(0)).toBe(CypherValueType.Node);
      expect(stmt.columnType(1)).toBe(CypherValueType.Relationship);
      expect(stmt.columnType(2)).toBe(CypherValueType.Node);

      const row = stmt.currentRow();
      expect(typeof row.a).toBe('bigint');
      expect(row.r).toMatchObject({
        subjectId: expect.anything(),
        predicateId: expect.anything(),
        objectId: expect.anything(),
      });
      expect(typeof row.b).toBe('bigint');
    }

    expect(rows).toBeGreaterThan(0);

    stmt.finalize();
    expect(() => stmt.step()).toThrow();

    await db.close();
  });
});

