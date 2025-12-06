import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { loadNativeBinding } from './helpers/native-binding.js';

const { open } = loadNativeBinding<{ open: (options: { dataPath: string }) => any }>();

describe('BigInt Support Tests', () => {
  const testDbPath = join(process.cwd(), 'test_bigint.redb');
  let db: any;

  beforeEach(() => {
    // Clean up existing test database
    if (existsSync(testDbPath)) {
      rmSync(testDbPath, { force: true });
    }

    // Open database - NAPI functions throw on error
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

  it('should handle bigint IDs in type system', () => {
    // Test 1: intern() returns bigint
    const id1 = db.intern('test_string_1');
    expect(typeof id1).toBe('bigint');
    expect(id1).toBeGreaterThanOrEqual(0n);

    // Test 2: resolveId() returns bigint | undefined
    const id2 = db.resolveId('test_string_1');
    expect(typeof id2).toBe('bigint');
    expect(id2).toBe(id1);

    // Test 3: resolveStr() accepts bigint
    const str = db.resolveStr(id1);
    expect(str).toBe('test_string_1');

    // Test 4: getDictionarySize() returns bigint
    const size = db.getDictionarySize();
    expect(typeof size).toBe('bigint');
    expect(size).toBeGreaterThan(0n);
  });

  it('should handle large bigint values (beyond u32::MAX)', () => {
    // u32::MAX = 4294967295
    const u32Max = 4294967295n;

    // Test with values larger than u32::MAX
    const largeId = u32Max + 1000n;

    // Test setNodeProperty with large ID
    db.setNodeProperty(largeId, JSON.stringify({ test: 'data' }));

    // Test getNodeProperty with large ID
    const prop = db.getNodeProperty(largeId);
    expect(prop).toBe(JSON.stringify({ test: 'data' }));

    // Test setEdgeProperty with large IDs
    const largeId2 = u32Max + 2000n;
    const largeId3 = u32Max + 3000n;
    db.setEdgeProperty(largeId, largeId2, largeId3, JSON.stringify({ edge: 'property' }));

    // Test getEdgeProperty with large IDs
    const edgeProp = db.getEdgeProperty(largeId, largeId2, largeId3);
    expect(edgeProp).toBe(JSON.stringify({ edge: 'property' }));
  });

  it('should handle TripleOutput with bigint IDs', () => {
    // Add a fact
    const triple = db.addFact('Alice', 'knows', 'Bob');

    // Verify all IDs are bigint
    expect(typeof triple.subjectId).toBe('bigint');
    expect(typeof triple.predicateId).toBe('bigint');
    expect(typeof triple.objectId).toBe('bigint');

    // All IDs should be >= 0
    expect(triple.subjectId).toBeGreaterThanOrEqual(0n);
    expect(triple.predicateId).toBeGreaterThanOrEqual(0n);
    expect(triple.objectId).toBeGreaterThanOrEqual(0n);
  });

  it('should handle QueryCriteriaInput with bigint', () => {
    // Add facts
    db.addFact('Alice', 'knows', 'Bob');
    db.addFact('Alice', 'likes', 'Charlie');

    // Get Alice's ID
    const aliceId = db.resolveId('Alice');
    expect(typeof aliceId).toBe('bigint');

    // Query with bigint criteria
    const results = db.query({ subjectId: aliceId });
    expect(Array.isArray(results)).toBe(true);
    expect(results.length).toBe(2);

    // All results should have bigint IDs
    for (const triple of results) {
      expect(typeof triple.subjectId).toBe('bigint');
      expect(typeof triple.predicateId).toBe('bigint');
      expect(typeof triple.objectId).toBe('bigint');
    }
  });

  it('should handle cursor operations with bigint', () => {
    // Add facts
    db.addFact('Alice', 'knows', 'Bob');
    db.addFact('Alice', 'likes', 'Charlie');

    // Get Alice's ID
    const aliceId = db.resolveId('Alice');

    // Open cursor with bigint criteria
    const cursor = db.openCursor({ subjectId: aliceId });
    expect(typeof cursor.id).toBe('bigint');

    // Read from cursor
    const batch = db.readCursor(cursor.id, 10);
    expect(batch.triples.length).toBe(2);

    // Close cursor with bigint ID
    db.closeCursor(cursor.id);
  });

  it('should correctly convert between number and bigint ranges', () => {
    // Test boundary values
    const maxSafeInteger = BigInt(Number.MAX_SAFE_INTEGER); // 2^53 - 1
    const u32Max = 4294967295n;

    // Number.MAX_SAFE_INTEGER is much larger than u32::MAX, but floats lose precision past 2^53
    expect(maxSafeInteger).toBeGreaterThan(u32Max);

    // Test property operations with value beyond Number.MAX_SAFE_INTEGER
    const largeId = maxSafeInteger + 1000n;
    db.setNodeProperty(largeId, '{}');

    // Verify we can retrieve it
    const result = db.getNodeProperty(largeId);
    expect(result).toBe('{}');
  });

  it('should maintain precision for large bigint values', () => {
    // Create a very large ID (within i64 range but beyond u32)
    const u32Max = 4294967295n;
    const largeId1 = u32Max + 123456789n;
    const largeId2 = u32Max + 987654321n;

    // Set properties with these large IDs
    db.setNodeProperty(largeId1, JSON.stringify({ id: 1 }));
    db.setNodeProperty(largeId2, JSON.stringify({ id: 2 }));

    // Verify we get different properties back (no collision)
    const prop1 = db.getNodeProperty(largeId1);
    const prop2 = db.getNodeProperty(largeId2);

    expect(prop1).not.toBe(prop2);
    expect(JSON.parse(prop1).id).toBe(1);
    expect(JSON.parse(prop2).id).toBe(2);
  });

  it('should handle batch add facts with bigint IDs', () => {
    // Intern some strings to get bigint IDs
    const aliceId = db.intern('Alice');
    const bobId = db.intern('Bob');
    const charlieId = db.intern('Charlie');
    const knowsId = db.intern('knows');
    const likesId = db.intern('likes');

    // Verify all IDs are bigint
    expect(typeof aliceId).toBe('bigint');
    expect(typeof bobId).toBe('bigint');
    expect(typeof charlieId).toBe('bigint');
    expect(typeof knowsId).toBe('bigint');
    expect(typeof likesId).toBe('bigint');

    // Batch add facts
    const triples = [
      { subjectId: aliceId, predicateId: knowsId, objectId: bobId },
      { subjectId: aliceId, predicateId: likesId, objectId: charlieId },
    ];

    const count = db.batchAddFacts(triples);

    // Verify return value is bigint
    expect(typeof count).toBe('bigint');
    expect(count).toBe(2n);

    // Verify facts were actually added
    const results = db.query({ subjectId: aliceId });
    expect(results.length).toBe(2);
  });

  it('should handle batch delete facts with bigint IDs', () => {
    // Add some facts first
    db.addFact('X', 'rel1', 'Y');
    db.addFact('X', 'rel2', 'Z');

    // Get IDs
    const xId = db.resolveId('X');
    const rel1Id = db.resolveId('rel1');
    const rel2Id = db.resolveId('rel2');
    const yId = db.resolveId('Y');
    const zId = db.resolveId('Z');

    expect(typeof xId).toBe('bigint');
    expect(typeof rel1Id).toBe('bigint');
    expect(typeof rel2Id).toBe('bigint');
    expect(typeof yId).toBe('bigint');
    expect(typeof zId).toBe('bigint');

    // Batch delete facts
    const triples = [
      { subjectId: xId!, predicateId: rel1Id!, objectId: yId! },
      { subjectId: xId!, predicateId: rel2Id!, objectId: zId! },
    ];

    const count = db.batchDeleteFacts(triples);

    // Verify return value is bigint
    expect(typeof count).toBe('bigint');
    expect(count).toBe(2n);

    // Verify facts were actually deleted
    const results = db.query({ subjectId: xId });
    expect(results.length).toBe(0);
  });

  it('should handle empty batch operations', () => {
    // Empty batch add
    const addCount = db.batchAddFacts([]);
    expect(typeof addCount).toBe('bigint');
    expect(addCount).toBe(0n);

    // Empty batch delete
    const deleteCount = db.batchDeleteFacts([]);
    expect(typeof deleteCount).toBe('bigint');
    expect(deleteCount).toBe(0n);
  });

  it('should handle batch operations with large bigint IDs', () => {
    const u32Max = 4294967295n;
    const largeId1 = u32Max + 1000n;
    const largeId2 = u32Max + 2000n;
    const largeId3 = u32Max + 3000n;

    // Batch add with large IDs
    const triples = [{ subjectId: largeId1, predicateId: largeId2, objectId: largeId3 }];

    const count = db.batchAddFacts(triples);
    expect(typeof count).toBe('bigint');
    expect(count).toBe(1n);

    // Verify the fact was added
    const results = db.query({ subjectId: largeId1 });
    expect(results.length).toBe(1);
    expect(results[0].subjectId).toBe(largeId1);
    expect(results[0].predicateId).toBe(largeId2);
    expect(results[0].objectId).toBe(largeId3);

    // Batch delete with large IDs
    const deleteCount = db.batchDeleteFacts(triples);
    expect(deleteCount).toBe(1n);

    // Verify the fact was deleted
    const afterDelete = db.query({ subjectId: largeId1 });
    expect(afterDelete.length).toBe(0);
  });
});
