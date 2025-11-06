import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { TemporalMemoryStore } from '../../../src/core/storage/temporal/temporalStore.js';
import { TemporalMemoryIngestor } from '../../../src/memory/temporal/ingestor.js';

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
