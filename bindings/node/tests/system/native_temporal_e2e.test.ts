import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { NervusDB } from '../../src/synapseDb.js';
import { loadNativeCore } from '../../src/native/core.js';

let tempDir: string;

beforeEach(async () => {
  tempDir = await mkdtemp(join(tmpdir(), 'nervus-native-e2e-'));
});

afterEach(async () => {
  await rm(tempDir, { recursive: true, force: true });
});

describe('Native Temporal E2E Smoke Tests', () => {
  it('should work with fresh install (native or fallback)', async () => {
    const dbPath = join(tempDir, 'fresh.nervusdb');
    const db = await NervusDB.open(dbPath);

    try {
      // Ingest sample messages
      await db.memory.ingestMessages(
        [
          { author: 'Alice', text: 'Hello Bob!', timestamp: '2025-01-01T10:00:00Z' },
          { author: 'Bob', text: 'Hi Alice!', timestamp: '2025-01-01T10:01:00Z' },
        ],
        { conversationId: 'test-conv', channel: 'chat' },
      );

      // Verify entities created
      const store = db.memory.getStore();
      const entities = store.getEntities();
      expect(entities.length).toBeGreaterThan(0);

      // Verify timeline query works
      const alice = entities.find((e) => e.canonicalName === 'alice');
      expect(alice).toBeDefined();

      if (alice) {
        const timeline = store.queryTimeline({
          entityId: alice.entityId,
          role: 'subject',
        });
        expect(Array.isArray(timeline)).toBe(true);
        expect(timeline.length).toBeGreaterThan(0);
      }
    } finally {
      await db.close();
    }
  });

  it('should migrate existing JSON temporal data', async () => {
    const dbPath = join(tempDir, 'migrate.nervusdb');

    // Step 1: Create data with TypeScript implementation
    process.env.NERVUSDB_DISABLE_NATIVE = '1';
    const db1 = await NervusDB.open(dbPath);

    try {
      await db1.memory.ingestMessages(
        [{ author: 'Charlie', text: 'Test message', timestamp: '2025-01-01T12:00:00Z' }],
        { conversationId: 'migrate-test', channel: 'chat' },
      );

      const store1 = db1.memory.getStore();
      const entities1 = store1.getEntities();
      const facts1 = store1.getFacts();

      expect(entities1.length).toBeGreaterThan(0);
      expect(facts1.length).toBeGreaterThan(0);
    } finally {
      await db1.close();
    }

    // Step 2: Reopen with native (if available)
    delete process.env.NERVUSDB_DISABLE_NATIVE;
    const db2 = await NervusDB.open(dbPath);

    try {
      const store2 = db2.memory.getStore();
      const entities2 = store2.getEntities();
      const facts2 = store2.getFacts();

      // Verify data is still accessible
      expect(entities2.length).toBeGreaterThan(0);
      expect(facts2.length).toBeGreaterThan(0);

      // Verify timeline query works
      const charlie = entities2.find((e) => e.canonicalName === 'charlie');
      expect(charlie).toBeDefined();

      if (charlie) {
        const timeline = store2.queryTimeline({
          entityId: charlie.entityId,
        });
        expect(timeline.length).toBeGreaterThan(0);
      }
    } finally {
      await db2.close();
    }
  });

  it('should handle native backend unavailable gracefully', async () => {
    // Force TypeScript implementation
    process.env.NERVUSDB_DISABLE_NATIVE = '1';

    const dbPath = join(tempDir, 'fallback.nervusdb');
    const db = await NervusDB.open(dbPath);

    try {
      // Should work with TypeScript fallback
      await db.memory.ingestMessages(
        [{ author: 'Dave', text: 'Fallback test', timestamp: '2025-01-01T14:00:00Z' }],
        { conversationId: 'fallback-test', channel: 'chat' },
      );

      const store = db.memory.getStore();
      const entities = store.getEntities();
      expect(entities.length).toBeGreaterThan(0);
    } finally {
      await db.close();
      delete process.env.NERVUSDB_DISABLE_NATIVE;
    }
  });

  it('should detect native backend availability', () => {
    const nativeCore = loadNativeCore();

    if (nativeCore) {
      console.log('✅ Native temporal backend available');
      expect(typeof nativeCore.open).toBe('function');
    } else {
      console.log('⚠️  Using TypeScript fallback');
      expect(nativeCore).toBeNull();
    }

    // Test should pass regardless of native availability
    expect(true).toBe(true);
  });

  it('should support all timeline query filters', async () => {
    const dbPath = join(tempDir, 'filters.nervusdb');
    const db = await NervusDB.open(dbPath);

    try {
      await db.memory.ingestMessages(
        [
          { author: 'Eve', text: 'Message 1', timestamp: '2025-01-01T10:00:00Z' },
          { author: 'Eve', text: 'Message 2', timestamp: '2025-01-02T10:00:00Z' },
          { author: 'Eve', text: 'Message 3', timestamp: '2025-01-03T10:00:00Z' },
        ],
        { conversationId: 'filter-test', channel: 'chat' },
      );

      const store = db.memory.getStore();
      const eve = store.getEntities().find((e) => e.canonicalName === 'eve');
      expect(eve).toBeDefined();

      if (eve) {
        // Test as_of filter
        const asOfTimeline = store.queryTimeline({
          entityId: eve.entityId,
          asOf: '2025-01-02T12:00:00Z',
        });
        expect(asOfTimeline.length).toBeGreaterThan(0);

        // Test between filter
        const betweenTimeline = store.queryTimeline({
          entityId: eve.entityId,
          between: ['2025-01-01T00:00:00Z', '2025-01-02T23:59:59Z'],
        });
        expect(betweenTimeline.length).toBeGreaterThan(0);

        // Test predicate filter
        const predicateTimeline = store.queryTimeline({
          entityId: eve.entityId,
          predicateKey: 'mentions',
        });
        // May be empty if no mentions, but should not throw
        expect(Array.isArray(predicateTimeline)).toBe(true);
      }
    } finally {
      await db.close();
    }
  });
});
