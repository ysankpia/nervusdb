import {
  TemporalMemoryStore,
  type TimestampInput,
} from '../../core/storage/temporal/temporalStore.js';
import { extractEntities } from './extractor.js';

export interface MessageInput {
  author: string;
  text: string;
  timestamp?: TimestampInput;
  metadata?: Record<string, unknown>;
}

export interface ConversationContext {
  conversationId?: string;
  channel?: string;
  sourceType?: string;
}

function toCanonicalName(value: string): string {
  return value.trim().toLowerCase();
}

function normaliseTimestamp(ts?: TimestampInput): Date {
  if (!ts) return new Date();
  return ts instanceof Date ? ts : new Date(ts);
}

export class TemporalMemoryIngestor {
  constructor(private readonly store: TemporalMemoryStore) {}

  async ingestMessages(messages: MessageInput[], context: ConversationContext = {}): Promise<void> {
    for (const message of messages) {
      const occurred = normaliseTimestamp(message.timestamp);
      const episode = await this.store.addEpisode({
        sourceType: context.sourceType ?? 'conversation',
        payload: {
          conversationId: context.conversationId ?? null,
          channel: context.channel ?? null,
          author: message.author,
          text: message.text,
          metadata: message.metadata ?? {},
        },
        occurredAt: occurred,
      });

      const authorCanonical = toCanonicalName(message.author);
      const authorEntity = await this.store.ensureEntity('agent', authorCanonical, {
        alias: message.author,
        occurredAt: occurred,
      });
      await this.store.linkEpisode(episode.episodeId, {
        entityId: authorEntity.entityId,
        role: 'author',
      });

      const mentions = extractEntities(message.text);
      for (const mention of mentions) {
        const mentionEntity = await this.store.ensureEntity(mention.kind, mention.canonical, {
          alias: mention.alias ?? mention.original,
          occurredAt: occurred,
        });
        await this.store.linkEpisode(episode.episodeId, {
          entityId: mentionEntity.entityId,
          role: 'mention',
        });

        const fact = await this.store.upsertFact({
          subjectEntityId: authorEntity.entityId,
          predicateKey: 'mentions',
          objectEntityId: mentionEntity.entityId,
          validFrom: occurred,
          sourceEpisodeId: episode.episodeId,
        });

        await this.store.linkEpisode(episode.episodeId, {
          factId: fact.factId,
          role: 'fact',
        });
      }
    }
  }
}
