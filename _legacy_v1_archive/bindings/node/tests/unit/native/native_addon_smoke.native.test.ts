import { afterEach, describe, expect, it } from 'vitest';

import { __setNativeCoreForTesting } from '../../../src/native/core.js';
import { NervusDB } from '../../../src/nervusDb.js';

const describeNative =
  process.env.NERVUSDB_EXPECT_NATIVE === '1' ? describe : describe.skip;

describeNative('native addon smoke', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;
  });

  it('opens database and performs basic add/query roundtrip', async () => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;

    const dbPath = `tmp-native-addon-smoke-${process.pid}-${Date.now()}`;
    const db = await NervusDB.open(dbPath);

    db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
    const facts = db.listFacts();

    expect(facts.length).toBe(1);
    expect(facts[0]).toMatchObject({
      subject: 'Alice',
      predicate: 'knows',
      object: 'Bob',
    });

    await db.close();
  });
});

