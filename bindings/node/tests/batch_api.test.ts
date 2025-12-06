import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';

import { loadNativeBinding } from './helpers/native-binding.js';

const { open } = loadNativeBinding<{ open: (options: { dataPath: string }) => any }>();

describe('Batch API', () => {
  const testDbPath = join(process.cwd(), 'test_batch_api.redb');
  let db: any;

  beforeEach(() => {
    if (existsSync(testDbPath)) {
      rmSync(testDbPath, { force: true });
    }
    db = open({ dataPath: testDbPath });
  });

  afterEach(() => {
    if (db) {
      db.close();
    }
    if (existsSync(testDbPath)) {
      rmSync(testDbPath, { force: true });
    }
  });

  it('batchAddFacts inserts multiple triples and batchDeleteFacts removes them', () => {
    const alice = db.intern('Alice');
    const bob = db.intern('Bob');
    const carol = db.intern('Carol');
    const knows = db.intern('knows');
    const likes = db.intern('likes');

    const inserted = db.batchAddFacts([
      { subjectId: alice, predicateId: knows, objectId: bob },
      { subjectId: alice, predicateId: likes, objectId: carol },
    ]);

    expect(typeof inserted).toBe('bigint');
    expect(inserted).toBe(2n);

    const results = db.query({ subjectId: alice });
    expect(results.length).toBe(2);

    const deleted = db.batchDeleteFacts([
      { subjectId: alice, predicateId: likes, objectId: carol },
    ]);

    expect(typeof deleted).toBe('bigint');
    expect(deleted).toBe(1n);

    const remaining = db.query({ subjectId: alice });
    expect(remaining.length).toBe(1);
    expect(remaining[0]?.predicateId).toBe(knows);
    expect(remaining[0]?.objectId).toBe(bob);
  });
});
