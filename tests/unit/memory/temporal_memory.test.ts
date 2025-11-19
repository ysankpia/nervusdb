import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { TemporalMemoryStore } from '../../../src/core/storage/temporal/temporalStore.js';
import { TemporalMemoryIngestor } from '../../../src/memory/temporal/ingestor.js';
import { NervusDB } from '../../../src/index.js';

let tempDir: string;
let basePath: string;

beforeEach(async () => {
  tempDir = await mkdtemp(join(tmpdir(), 'nervus-temporal-'));
  basePath = join(tempDir, 'testdb.synapsedb');
});

afterEach(async () => {
  await rm(tempDir, { recursive: true, force: true });
});

describe('TemporalMemoryStore', () => {
  it('deduplicates episodes based on trace hash', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const episode1 = await store.addEpisode({
      sourceType: 'test',
      payload: { foo: 'bar' },
      occurredAt: '2025-01-01T00:00:00.000Z',
    });
    const episode2 = await store.addEpisode({
      sourceType: 'test',
      payload: { foo: 'bar' },
      occurredAt: '2025-01-01T00:00:00.000Z',
    });
    expect(episode1.episodeId).toBe(episode2.episodeId);
    expect(store.getEpisodes()).toHaveLength(1);
    await store.close();
  });

  it('filters timeline by predicate, role and temporal bounds', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const episode = await store.addEpisode({
      sourceType: 'seed',
      payload: { note: 'seed' },
      occurredAt: '2025-01-01T00:00:00Z',
    });
    const alice = await store.ensureEntity('agent', 'Alice', {
      occurredAt: '2025-01-01T00:00:00Z',
    });
    const bob = await store.ensureEntity('agent', 'Bob', {
      occurredAt: '2025-01-01T01:00:00Z',
    });
    await store.upsertFact({
      subjectEntityId: alice.entityId,
      predicateKey: 'knows',
      objectEntityId: bob.entityId,
      validFrom: '2025-01-02T00:00:00Z',
      validTo: '2025-01-10T00:00:00Z',
      sourceEpisodeId: episode.episodeId,
    });
    await store.upsertFact({
      subjectEntityId: alice.entityId,
      predicateKey: 'knows',
      objectEntityId: bob.entityId,
      validFrom: '2025-02-01T00:00:00Z',
      validTo: null,
      sourceEpisodeId: episode.episodeId,
    });

    const midJanuary = store.queryTimeline({
      entityId: alice.entityId,
      predicateKey: 'knows',
      asOf: '2025-01-05T00:00:00Z',
    });
    expect(midJanuary).toHaveLength(1);

    const march = store.queryTimeline({
      entityId: alice.entityId,
      predicateKey: 'knows',
      asOf: '2025-03-05T00:00:00Z',
    });
    expect(march).toHaveLength(1);

    const beforeStart = store.queryTimeline({
      entityId: alice.entityId,
      predicateKey: 'knows',
      asOf: '2024-12-31T23:00:00Z',
    });
    expect(beforeStart).toHaveLength(0);

    const betweenRange = store.queryTimeline({
      entityId: bob.entityId,
      role: 'object',
      between: ['2025-01-03T00:00:00Z', '2025-01-15T00:00:00Z'],
    });
    expect(betweenRange).toHaveLength(1);

    await store.close();
  });

  it('persists memory data to disk between restarts', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const episode = await store.addEpisode({
      sourceType: 'persist',
      payload: { marker: 'persist' },
      occurredAt: '2025-01-01T00:00:00Z',
    });
    const entity = await store.ensureEntity('agent', 'Persisted', {
      occurredAt: '2025-01-01T00:00:00Z',
    });
    await store.upsertFact({
      subjectEntityId: entity.entityId,
      predicateKey: 'notes',
      objectValue: 'persisted fact',
      validFrom: '2025-01-01T00:00:00Z',
      sourceEpisodeId: episode.episodeId,
    });
    await store.close();

    const reopened = await TemporalMemoryStore.initialize(basePath);
    expect(reopened.getEpisodes()).toHaveLength(1);
    expect(reopened.getEntities()).toHaveLength(1);
    expect(reopened.getFacts()).toHaveLength(1);
    await reopened.close();
  });

  it('increments entity version and maintains seen timestamps', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const first = await store.ensureEntity('agent', 'Delta', {
      occurredAt: '2025-04-01T00:00:00Z',
    });
    const initialVersion = first.version;
    const updated = await store.ensureEntity('agent', 'Delta', {
      occurredAt: '2025-03-01T00:00:00Z',
      versionIncrement: true,
    });
    expect(updated.entityId).toBe(first.entityId);
    expect(updated.version).toBe(initialVersion + 1);
    expect(updated.firstSeen).toBe('2025-03-01T00:00:00.000Z');
    expect(updated.lastSeen).toBe('2025-04-01T00:00:00.000Z');
    await store.close();
  });

  it('merges duplicate facts and closes intervals', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const episode = await store.addEpisode({
      sourceType: 'merge',
      payload: {},
      occurredAt: '2025-01-01T00:00:00Z',
    });
    const entity = await store.ensureEntity('agent', 'Node', {
      occurredAt: '2025-01-01T00:00:00Z',
    });
    const first = await store.upsertFact({
      subjectEntityId: entity.entityId,
      predicateKey: 'seen',
      objectValue: 'alpha',
      validFrom: '2025-02-01T00:00:00Z',
      sourceEpisodeId: episode.episodeId,
    });
    const merged = await store.upsertFact({
      subjectEntityId: entity.entityId,
      predicateKey: 'seen',
      objectValue: 'alpha',
      validFrom: '2025-01-15T00:00:00Z',
      validTo: '2025-03-01T00:00:00Z',
      confidence: 0.5,
      sourceEpisodeId: episode.episodeId,
    });
    expect(merged.factId).toBe(first.factId);
    expect(merged.validFrom).toBe('2025-01-15T00:00:00.000Z');
    expect(merged.validTo).toBe('2025-03-01T00:00:00.000Z');
    expect(merged.confidence).toBe(0.5);
    expect(store.getFacts()).toHaveLength(1);
    await store.close();
  });

  it('reuses existing links and handles missing tracebacks', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const episode = await store.addEpisode({
      sourceType: 'link',
      payload: {},
      occurredAt: '2025-01-01T00:00:00Z',
    });
    const fact = await store.upsertFact({
      subjectEntityId: 1,
      predicateKey: 'knows',
      objectEntityId: 2,
      sourceEpisodeId: episode.episodeId,
    });
    expect(store.traceBack(fact.factId + 1)).toEqual([]);

    const link = await store.linkEpisode(episode.episodeId, { factId: fact.factId, role: 'fact' });
    const again = await store.linkEpisode(episode.episodeId, {
      factId: fact.factId,
      role: 'fact',
    });
    expect(again.linkId).toBe(link.linkId);
    expect(store.traceBack(fact.factId)).toHaveLength(1);
    await store.close();
  });
});

describe('TemporalMemoryIngestor', () => {
  it('ingests conversation messages and creates timeline facts', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);
    const ingestor = new TemporalMemoryIngestor(store);

    await ingestor.ingestMessages(
      [
        {
          author: 'Alice',
          text: 'Met with Bob at #OpenAI HQ today.',
          timestamp: '2025-02-01T10:00:00Z',
        },
        {
          author: 'Bob',
          text: 'Thanks Alice! See you soon.',
          timestamp: '2025-02-01T11:00:00Z',
        },
      ],
      { conversationId: 'conv-1', channel: 'chat' },
    );

    const entities = store.getEntities();
    const alice = entities.find((entity) => entity.canonicalName === 'alice');
    expect(alice).toBeDefined();

    const facts = store.queryTimeline({ entityId: alice!.entityId });
    expect(facts.length).toBeGreaterThan(0);

    const trace = store.traceBack(facts[0].factId);
    expect(trace.length).toBeGreaterThan(0);
    const authors = trace.map((episode) => (episode.payload as any).author);
    expect(authors).toContain('Alice');

    await store.close();
  });
});

describe('NervusDB memory facade', () => {
  it('exposes temporal helpers through the public API', async () => {
    const db = await NervusDB.open(':memory:');

    try {
      const episode = await db.memory.addEpisode({
        sourceType: 'test',
        payload: { kind: 'note' },
        occurredAt: '2025-03-01T09:00:00Z',
      });

      const entity = await db.memory.ensureEntity('agent', 'alice', {
        alias: 'Alice',
        occurredAt: '2025-03-01T09:00:00Z',
      });

      const fact = await db.memory.upsertFact({
        subjectEntityId: entity.entityId,
        predicateKey: 'mentions',
        objectValue: 'graph databases',
        validFrom: '2025-03-01T09:00:00Z',
        sourceEpisodeId: episode.episodeId,
      });

      await db.memory.linkEpisode(episode.episodeId, { factId: fact.factId, role: 'fact' });

      const timeline = db.memory.timeline({ entityId: entity.entityId });
      expect(timeline).toHaveLength(1);

      const traced = db.memory.traceBack(fact.factId);
      expect(traced.some((ep) => ep.episodeId === episode.episodeId)).toBe(true);

      const timelineFacts = db.memory.timelineBuilder(entity.entityId).predicate('mentions').all();
      expect(timelineFacts).toHaveLength(1);

      const timelineAsOf = db.memory
        .timelineBuilder(entity.entityId)
        .asOf('2025-03-01T09:00:00Z')
        .all();
      expect(timelineAsOf).toHaveLength(1);
    } finally {
      await db.close();
    }
  });
});
