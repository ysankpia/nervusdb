#!/usr/bin/env node
import { NervusDB, TemporalMemoryIngestor } from '../dist/index.mjs';
import { readFile } from 'node:fs/promises';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

async function loadJson(relativePath) {
  const url = new URL(relativePath, import.meta.url);
  const raw = await readFile(url, 'utf8');
  return JSON.parse(raw);
}

function mapEntities(store) {
  const entities = store.getEntities();
  const byId = new Map();
  const byCanonicalKind = new Map();
  for (const entity of entities) {
    byId.set(entity.entityId, entity.canonicalName);
    const key = `${entity.kind}:${entity.canonicalName}`;
    if (!byCanonicalKind.has(key)) {
      byCanonicalKind.set(key, entity);
    }
  }
  return { byId, byCanonicalKind };
}

function evaluateDmr(store, dataset) {
  const { byId, byCanonicalKind } = mapEntities(store);
  const results = [];
  for (const sample of dataset) {
    for (const query of sample.queries) {
      const entity = byCanonicalKind.get(`${query.kind ?? 'agent'}:${query.entity}`);
      if (!entity) {
        results.push({ id: sample.id, accuracy: 0 });
        continue;
      }
      const facts = store.queryTimeline({ entityId: entity.entityId, predicateKey: query.predicate });
      const objects = new Set(
        facts
          .map((fact) => byId.get(fact.objectEntityId ?? -1))
          .filter((name) => Boolean(name)),
      );
      let hits = 0;
      for (const expected of query.expected) {
        if (objects.has(expected)) hits += 1;
      }
      results.push({ id: sample.id, accuracy: hits / query.expected.length });
    }
  }
  return results;
}

function evaluateLongMem(store, dataset) {
  const { byId } = mapEntities(store);
  const results = [];
  for (const sample of dataset) {
    for (const check of sample.checks) {
      const entity = store.getEntities().find((ent) => ent.canonicalName === check.entity);
      if (!entity) {
        results.push({ id: sample.id, check: 'missing-entity', ok: false });
        continue;
      }
      const timeline = store.queryTimeline({
        entityId: entity.entityId,
        predicateKey: check.predicate,
        asOf: check.asOf,
        between: check.between,
        role: check.role,
      });
      const objects = new Set(
        timeline
          .map((fact) => byId.get(fact.objectEntityId ?? -1))
          .filter((name) => Boolean(name)),
      );
      const expectations = check.expectContains ?? [];
      const misses = expectations.filter((item) => !objects.has(item));
      results.push({
        id: sample.id,
        check: check.asOf ? 'asOf' : 'between',
        observed: Array.from(objects),
        expected: expectations,
        ok: misses.length === 0,
      });
    }
  }
  return results;
}

async function ingestConversation(ingestor, messages) {
  await ingestor.ingestMessages(
    messages.map((msg) => ({
      author: msg.author,
      text: msg.text,
      timestamp: msg.timestamp ?? msg.occurredAt,
    })),
  );
}

async function main() {
  const tmpDir = await mkdtemp(join(tmpdir(), 'nervus-temporal-bench-'));
  const db = await NervusDB.open(join(tmpDir, 'temporal.synapsedb'));
  const store = db.memory.getStore();
  if (!store) throw new Error('Temporal memory store missing');
  const ingestor = new TemporalMemoryIngestor(store);

  try {
    const dmrDataset = await loadJson('./data/dmr-sample.json');
    for (const sample of dmrDataset) {
      await ingestConversation(ingestor, sample.conversation);
    }
    const dmrSummary = evaluateDmr(store, dmrDataset);

    const longMemDataset = await loadJson('./data/longmemeval-sample.json');
    for (const sample of longMemDataset) {
      await ingestConversation(ingestor, sample.episodes);
    }
    const longMemSummary = evaluateLongMem(store, longMemDataset);

    console.log('=== DMR Sample Accuracy ===');
    console.table(dmrSummary);
    console.log('=== LongMemEval Sample Checks ===');
    console.table(longMemSummary);
  } finally {
    await db.close();
    await rm(tmpDir, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error('[temporal-memory-bench] failed:', error);
  process.exitCode = 1;
});
