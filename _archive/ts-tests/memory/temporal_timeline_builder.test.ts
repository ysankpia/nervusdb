import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { TemporalMemoryStore } from '../../../src/core/storage/temporal/temporalStore.js';
import { TemporalTimelineBuilder } from '../../../src/memory/temporal/timelineBuilder.js';

let tempDir: string;
let basePath: string;

beforeEach(async () => {
  tempDir = await mkdtemp(join(tmpdir(), 'nervus-temporal-'));
  basePath = join(tempDir, 'timeline.synapsedb');
});

afterEach(async () => {
  await rm(tempDir, { recursive: true, force: true });
});

describe('TemporalTimelineBuilder', () => {
  it('applies predicate, role, asOf and between filters', async () => {
    const store = await TemporalMemoryStore.initialize(basePath);

    const alice = await store.ensureEntity('agent', 'alice', {
      alias: 'Alice',
      occurredAt: '2025-02-01T10:00:00Z',
    });
    const bob = await store.ensureEntity('agent', 'bob', {
      alias: 'Bob',
      occurredAt: '2025-02-01T10:05:00Z',
    });

    const episode = await store.addEpisode({
      sourceType: 'conversation',
      payload: { text: 'Alice met Bob' },
      occurredAt: '2025-02-01T10:00:00Z',
    });

    const fact = await store.upsertFact({
      subjectEntityId: alice.entityId,
      predicateKey: 'mentions',
      objectEntityId: bob.entityId,
      validFrom: '2025-02-01T10:00:00Z',
      sourceEpisodeId: episode.episodeId,
    });

    await store.linkEpisode(episode.episodeId, {
      entityId: alice.entityId,
      role: 'author',
    });
    await store.linkEpisode(episode.episodeId, {
      factId: fact.factId,
      role: 'fact',
    });

    const builder = new TemporalTimelineBuilder(
      alice.entityId,
      (query) => store.queryTimeline(query),
      (factId) => store.traceBack(factId),
    );

    const allFacts = builder.all();
    expect(allFacts).toHaveLength(1);

    const predicateFacts = builder.predicate('mentions').all();
    expect(predicateFacts[0]?.factId).toBe(fact.factId);

    const noFacts = builder.roleAs('object').all();
    expect(noFacts).toHaveLength(0);

    const asOfFacts = builder.roleAs('subject').asOf('2025-02-01T10:00:00Z').all();
    expect(asOfFacts).toHaveLength(1);

    const betweenFacts = builder.between('2025-01-01T00:00:00Z', '2025-01-02T00:00:00Z').all();
    expect(betweenFacts).toHaveLength(0);

    const traced = builder.trace(fact.factId);
    expect(traced.some((ep) => ep.episodeId === episode.episodeId)).toBe(true);

    await store.close();
  });
});
