import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import { TemporalMemoryStore } from '../../../src/core/storage/temporal/temporalStore.js';
import { TemporalMemoryIngestor } from '../../../src/memory/temporal/ingestor.js';
import { loadNativeCore } from '../../../src/native/core.js';

const SAMPLE_MESSAGES = [
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
];

let tempDir: string;

beforeEach(async () => {
  tempDir = await mkdtemp(join(tmpdir(), 'nervus-native-parity-'));
});

afterEach(async () => {
  await rm(tempDir, { recursive: true, force: true });
});

describe('native temporal backend parity', () => {
  it('matches JSON backend for ingest + timeline queries when native is available', async () => {
    const binding = loadNativeCore();
    if (!binding) {
      // No native addon built in this environment – skip parity assertion.
      return;
    }

    const nativePath = join(tempDir, 'native.synapsedb');
    const jsonPath = join(tempDir, 'json.synapsedb');

    const nativeHandle = binding.open({ dataPath: nativePath });
    const nativeStore = await TemporalMemoryStore.initialize(nativePath, nativeHandle);
    const jsonStore = await TemporalMemoryStore.initialize(jsonPath);

    try {
      const nativeIngestor = new TemporalMemoryIngestor(nativeStore);
      const jsonIngestor = new TemporalMemoryIngestor(jsonStore);

      await nativeIngestor.ingestMessages(SAMPLE_MESSAGES, {
        conversationId: 'conv-1',
        channel: 'chat',
      });
      await jsonIngestor.ingestMessages(SAMPLE_MESSAGES, {
        conversationId: 'conv-1',
        channel: 'chat',
      });

      const nativeEntities = nativeStore.getEntities();
      const jsonEntities = jsonStore.getEntities();
      expect(normaliseEntities(nativeEntities)).toEqual(normaliseEntities(jsonEntities));

      const alice = jsonEntities.find((entity) => entity.canonicalName === 'alice');
      expect(alice).toBeDefined();
      const aliceId = alice?.entityId;
      if (!aliceId) throw new Error('alice entity not found');

      const query = {
        entityId: aliceId,
        predicateKey: 'mentions',
        role: 'subject' as const,
      };
      const nativeTimeline = nativeStore.queryTimeline(query);
      const jsonTimeline = jsonStore.queryTimeline(query);
      expect(normaliseFacts(nativeTimeline)).toEqual(normaliseFacts(jsonTimeline));

      const factId = jsonTimeline[0]?.factId;
      expect(factId).toBeDefined();
      if (!factId) throw new Error('timeline fact missing');

      const nativeTrace = nativeStore.traceBack(factId);
      const jsonTrace = jsonStore.traceBack(factId);
      expect(normaliseEpisodes(nativeTrace)).toEqual(normaliseEpisodes(jsonTrace));
    } finally {
      await nativeStore.close();
      await jsonStore.close();
      nativeHandle.close();
    }
  });
});

function normaliseEntities(entities: ReturnType<TemporalMemoryStore['getEntities']>) {
  return entities
    .map((entity) => ({
      canonicalName: entity.canonicalName,
      kind: entity.kind,
      version: entity.version,
    }))
    .sort((a, b) => a.canonicalName.localeCompare(b.canonicalName));
}

function normaliseFacts(facts: ReturnType<TemporalMemoryStore['queryTimeline']>) {
  return facts
    .map((fact) => ({
      subjectEntityId: fact.subjectEntityId,
      predicateKey: fact.predicateKey,
      objectEntityId: fact.objectEntityId,
      sourceEpisodeId: fact.sourceEpisodeId,
    }))
    .sort(
      (a, b) =>
        a.subjectEntityId - b.subjectEntityId || a.predicateKey.localeCompare(b.predicateKey),
    );
}

function normaliseEpisodes(episodes: ReturnType<TemporalMemoryStore['traceBack']>) {
  return episodes
    .map((episode) => ({
      sourceType: episode.sourceType,
      occurredAt: episode.occurredAt,
      author: (episode.payload as any)?.author,
    }))
    .sort((a, b) => a.occurredAt.localeCompare(b.occurredAt));
}
