import { describe, it, expect } from 'vitest';
import { StorageEngine } from '../src/wasm/nervusdb_wasm.js';

describe('WASM Storage Engine - Stress Tests', () => {
  it('should handle large dataset (10K records)', () => {
    const engine = new StorageEngine();
    const recordCount = 10000;

    // Insert 10K records
    for (let i = 0; i < recordCount; i++) {
      engine.insert(`subject_${i}`, `predicate_${i % 100}`, `object_${i}`);
    }

    expect(engine.size()).toBe(recordCount);

    // Query random subjects
    for (let i = 0; i < 100; i++) {
      const randomId = Math.floor(Math.random() * recordCount);
      const results = engine.query_by_subject(`subject_${randomId}`);
      expect(results).toHaveLength(1);
      expect(results[0].subject).toBe(`subject_${randomId}`);
    }

    engine.free();
  });

  it('should handle repeated insertions without memory leak', () => {
    const engine = new StorageEngine();

    // Insert and clear 5 times
    for (let round = 0; round < 5; round++) {
      for (let i = 0; i < 1000; i++) {
        engine.insert(`s${i}`, `p${i % 10}`, `o${i}`);
      }
      expect(engine.size()).toBe(1000);
      engine.clear();
      expect(engine.size()).toBe(0);
    }

    engine.free();
  });

  it('should handle concurrent queries efficiently', () => {
    const engine = new StorageEngine();

    // Setup: Insert 1000 records
    for (let i = 0; i < 1000; i++) {
      engine.insert(`person_${i}`, 'knows', `person_${i + 1}`);
    }

    // Execute 1000 queries
    const start = performance.now();
    for (let i = 0; i < 1000; i++) {
      const results = engine.query_by_subject(`person_${i}`);
      expect(results.length).toBeGreaterThan(0);
    }
    const elapsed = performance.now() - start;

    // Should complete 1000 queries in reasonable time
    // CI environments (2-core CPU) are slower than local dev
    const threshold = process.env.CI ? 2000 : 500;
    expect(elapsed).toBeLessThan(threshold); // 500ms local, 2000ms CI

    engine.free();
  });

  it('should handle edge cases gracefully', () => {
    const engine = new StorageEngine();

    // Empty queries
    expect(engine.query_by_subject('nonexistent')).toHaveLength(0);
    expect(engine.query_by_predicate('nonexistent')).toHaveLength(0);

    // Special characters
    engine.insert('subject with spaces', 'pred:colon', 'obj/slash');
    const results = engine.query_by_subject('subject with spaces');
    expect(results).toHaveLength(1);

    // Very long strings
    const longString = 'a'.repeat(1000);
    engine.insert(longString, 'predicate', 'object');
    const longResults = engine.query_by_subject(longString);
    expect(longResults).toHaveLength(1);

    // Unicode
    engine.insert('主题', '谓语', '宾语');
    const unicodeResults = engine.query_by_subject('主题');
    expect(unicodeResults).toHaveLength(1);

    engine.free();
  });

  it('should maintain consistency after multiple operations', () => {
    const engine = new StorageEngine();

    // Complex scenario: insert, query, clear, insert again
    for (let i = 0; i < 100; i++) {
      engine.insert(`s${i}`, `p${i % 5}`, `o${i}`);
    }

    const beforeClear = engine.size();
    expect(beforeClear).toBe(100);

    // Query by predicate
    const p0Results = engine.query_by_predicate('p0');
    expect(p0Results.length).toBe(20); // 100 / 5

    engine.clear();
    expect(engine.size()).toBe(0);

    // Insert different data
    for (let i = 0; i < 50; i++) {
      engine.insert(`new_s${i}`, `new_p`, `new_o${i}`);
    }

    expect(engine.size()).toBe(50);
    const newResults = engine.query_by_predicate('new_p');
    expect(newResults.length).toBe(50);

    engine.free();
  });

  it('should handle memory efficiently with large result sets', () => {
    const engine = new StorageEngine();

    // Insert 5000 records with same predicate
    const commonPredicate = 'common_pred';
    for (let i = 0; i < 5000; i++) {
      engine.insert(`s${i}`, commonPredicate, `o${i}`);
    }

    // Query should return all 5000
    const start = performance.now();
    const results = engine.query_by_predicate(commonPredicate);
    const elapsed = performance.now() - start;

    expect(results).toHaveLength(5000);
    // CI environments need more time for large result sets
    const threshold = process.env.CI ? 800 : 200;
    expect(elapsed).toBeLessThan(threshold); // 200ms local, 800ms CI

    engine.free();
  });
});
