import { describe, it, expect } from 'vitest';
import { StorageEngine } from '../src/wasm/nervusdb_wasm.js';

describe('WASM Storage Engine', () => {
  it('should create storage engine', () => {
    const engine = new StorageEngine();
    expect(engine).toBeDefined();
    expect(engine.size()).toBe(0);
  });

  it('should insert triples', () => {
    const engine = new StorageEngine();

    engine.insert('Alice', 'knows', 'Bob');
    engine.insert('Bob', 'knows', 'Charlie');
    engine.insert('Alice', 'likes', 'Coffee');

    expect(engine.size()).toBe(3);
  });

  it('should query by subject', () => {
    const engine = new StorageEngine();

    engine.insert('Alice', 'knows', 'Bob');
    engine.insert('Alice', 'likes', 'Coffee');
    engine.insert('Bob', 'knows', 'Charlie');

    const results = engine.query_by_subject('Alice');
    expect(results).toHaveLength(2);
    expect(results[0].subject).toBe('Alice');
  });

  it('should query by predicate', () => {
    const engine = new StorageEngine();

    engine.insert('Alice', 'knows', 'Bob');
    engine.insert('Bob', 'knows', 'Charlie');
    engine.insert('Alice', 'likes', 'Coffee');

    const results = engine.query_by_predicate('knows');
    expect(results).toHaveLength(2);
  });

  it('should get statistics', () => {
    const engine = new StorageEngine();

    engine.insert('Alice', 'knows', 'Bob');
    engine.insert('Bob', 'knows', 'Charlie');

    const stats = engine.get_stats();
    expect(stats).toBeDefined();
    // Stats is returned as an object
    expect(typeof stats).toBe('object');
  });

  it('should clear data', () => {
    const engine = new StorageEngine();

    engine.insert('Alice', 'knows', 'Bob');
    engine.insert('Bob', 'knows', 'Charlie');
    expect(engine.size()).toBe(2);

    engine.clear();
    expect(engine.size()).toBe(0);
  });

  it('should handle multiple queries', () => {
    const engine = new StorageEngine();

    // Insert test data
    for (let i = 0; i < 100; i++) {
      engine.insert(`Person${i}`, 'knows', `Person${i + 1}`);
    }

    expect(engine.size()).toBe(100);

    // Query
    const results = engine.query_by_subject('Person50');
    expect(results).toHaveLength(1);
    expect(results[0].object).toBe('Person51');

    // Stats object should be defined
    const stats = engine.get_stats();
    expect(stats).toBeDefined();
  });
});
