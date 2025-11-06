import { describe, it, expect } from 'vitest';
import { NervusDB } from '@/synapseDb';

const MESSAGES = [
  {
    author: 'Alice',
    text: 'Met with Bob and #Carol at the lab.',
    timestamp: '2025-05-01T09:00:00Z',
  },
  {
    author: 'Bob',
    text: 'Great catching up, Alice!',
    timestamp: '2025-05-01T09:05:00Z',
  },
  {
    author: 'Alice',
    text: "Let's invite Dave next time.",
    timestamp: '2025-05-01T09:10:00Z',
  },
];

describe('Temporal memory default integration', () => {
  it('supports ingesting conversations without manual wiring', async () => {
    const db = await NervusDB.open(':memory:');

    try {
      await db.memory.ingestMessages(MESSAGES, {
        conversationId: 'conv-1',
        channel: 'chat',
      });

      const store = db.memory.getStore();
      expect(store).toBeDefined();

      const alice = store!.getEntities().find((entity) => entity.canonicalName === 'alice');
      expect(alice).toBeDefined();

      const timeline = db.memory
        .timelineBuilder(alice!.entityId)
        .predicate('mentions')
        .roleAs('subject')
        .all();

      expect(timeline.length).toBeGreaterThan(0);

      const entities = store!.getEntities();
      const mentioned = new Set(
        timeline
          .map((fact) => {
            const entity = entities.find((candidate) => candidate.entityId === fact.objectEntityId);
            return entity?.canonicalName ?? null;
          })
          .filter((name): name is string => Boolean(name)),
      );

      expect(mentioned.has('bob')).toBe(true);
      expect(mentioned.has('carol')).toBe(true);
    } finally {
      await db.close();
    }
  });
});
