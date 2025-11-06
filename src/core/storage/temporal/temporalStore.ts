import { createHash } from 'node:crypto';
import { promises as fsp } from 'node:fs';
import { dirname } from 'node:path';

const DATA_SUFFIX = '.temporal.json';
const INFINITY_TS = '9999-12-31T23:59:59.999Z';

export type TimestampInput = Date | string | number;

export interface EpisodeInput {
  sourceType: string;
  payload: unknown;
  occurredAt: TimestampInput;
  traceHash?: string;
}

export interface StoredEpisode {
  episodeId: number;
  sourceType: string;
  payload: unknown;
  occurredAt: string;
  ingestedAt: string;
  traceHash: string;
}

export interface StoredEntity {
  entityId: number;
  kind: string;
  canonicalName: string;
  fingerprint: string;
  firstSeen: string;
  lastSeen: string;
  version: number;
}

export interface StoredAlias {
  aliasId: number;
  entityId: number;
  aliasText: string;
  confidence: number;
}

export interface FactWriteInput {
  subjectEntityId: number;
  predicateKey: string;
  objectEntityId?: number | null;
  objectValue?: unknown;
  validFrom?: TimestampInput;
  validTo?: TimestampInput | null;
  confidence?: number;
  sourceEpisodeId: number;
}

export interface StoredFact {
  factId: number;
  subjectEntityId: number;
  predicateKey: string;
  objectEntityId: number | null;
  objectValue: unknown;
  validFrom: string;
  validTo: string | null;
  confidence: number;
  sourceEpisodeId: number;
}

export interface EpisodeLinkRecord {
  linkId: number;
  episodeId: number;
  entityId: number | null;
  factId: number | null;
  role: string;
}

export interface TimelineQuery {
  entityId: number;
  predicateKey?: string;
  role?: 'subject' | 'object';
  asOf?: TimestampInput;
  between?: [TimestampInput, TimestampInput];
}

interface TemporalMemoryData {
  counters: {
    episode: number;
    entity: number;
    alias: number;
    fact: number;
    link: number;
  };
  episodes: StoredEpisode[];
  entities: StoredEntity[];
  aliases: StoredAlias[];
  facts: StoredFact[];
  links: EpisodeLinkRecord[];
}

export interface EnsureEntityOptions {
  alias?: string;
  confidence?: number;
  occurredAt?: TimestampInput;
  versionIncrement?: boolean;
}

function normaliseTimestamp(value: TimestampInput): string {
  const d = value instanceof Date ? value : new Date(value);
  return d.toISOString();
}

function canonicalFingerprint(kind: string, canonicalName: string): string {
  return `${kind}:${canonicalName.toLowerCase()}`;
}

function defaultData(): TemporalMemoryData {
  return {
    counters: {
      episode: 1,
      entity: 1,
      alias: 1,
      fact: 1,
      link: 1,
    },
    episodes: [],
    entities: [],
    aliases: [],
    facts: [],
    links: [],
  };
}

export class TemporalMemoryStore {
  static async initialize(dataPath: string): Promise<TemporalMemoryStore> {
    const filePath = `${dataPath}${DATA_SUFFIX}`;
    const dir = dirname(filePath);
    await fsp.mkdir(dir, { recursive: true });

    let payload: TemporalMemoryData;
    try {
      const raw = await fsp.readFile(filePath, 'utf8');
      const parsed = JSON.parse(raw) as Partial<TemporalMemoryData>;
      const initial = defaultData();
      payload = {
        counters: { ...initial.counters, ...(parsed.counters ?? {}) },
        episodes: parsed.episodes ?? [],
        entities: parsed.entities ?? [],
        aliases: parsed.aliases ?? [],
        facts: parsed.facts ?? [],
        links: parsed.links ?? [],
      };
    } catch (error: unknown) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        payload = defaultData();
        await fsp.writeFile(filePath, JSON.stringify(payload, null, 2), 'utf8');
      } else {
        throw error;
      }
    }
    return new TemporalMemoryStore(filePath, payload);
  }

  private dirty = false;

  private constructor(
    private readonly filePath: string,
    private readonly data: TemporalMemoryData,
  ) {}

  async close(): Promise<void> {
    if (this.dirty) {
      await this.persist();
    }
  }

  getEpisodes(): StoredEpisode[] {
    return [...this.data.episodes];
  }

  getEntities(): StoredEntity[] {
    return [...this.data.entities];
  }

  getFacts(): StoredFact[] {
    return [...this.data.facts];
  }

  async addEpisode(input: EpisodeInput): Promise<StoredEpisode> {
    const occurredAt = normaliseTimestamp(input.occurredAt);
    const ingestedAt = new Date().toISOString();
    const trace = input.traceHash ?? this.computeTraceHash(input, occurredAt);
    const existing = this.data.episodes.find((ep) => ep.traceHash === trace);
    if (existing) {
      return existing;
    }
    const episodeId = this.data.counters.episode++;
    const record: StoredEpisode = {
      episodeId,
      sourceType: input.sourceType,
      payload: input.payload,
      occurredAt,
      ingestedAt,
      traceHash: trace,
    };
    this.data.episodes.push(record);
    this.dirty = true;
    await this.persist();
    return record;
  }

  async ensureEntity(
    kind: string,
    canonicalName: string,
    options: EnsureEntityOptions = {},
  ): Promise<StoredEntity> {
    const fingerprint = canonicalFingerprint(kind, canonicalName);
    const occurredAt = options.occurredAt
      ? normaliseTimestamp(options.occurredAt)
      : new Date().toISOString();
    let entity = this.data.entities.find((ent) => ent.fingerprint === fingerprint);
    if (!entity) {
      const newEntity: StoredEntity = {
        entityId: this.data.counters.entity++,
        kind,
        canonicalName,
        fingerprint,
        firstSeen: occurredAt,
        lastSeen: occurredAt,
        version: 1,
      };
      this.data.entities.push(newEntity);
      entity = newEntity;
    } else {
      if (options.versionIncrement) {
        entity.version += 1;
      }
      entity.lastSeen = occurredAt > entity.lastSeen ? occurredAt : entity.lastSeen;
      entity.firstSeen = occurredAt < entity.firstSeen ? occurredAt : entity.firstSeen;
    }

    if (options.alias) {
      const aliasLower = options.alias.toLowerCase();
      const existingAlias = this.data.aliases.find((alias) => {
        return alias.entityId === entity.entityId && alias.aliasText.toLowerCase() === aliasLower;
      });
      if (!existingAlias) {
        const aliasRecord: StoredAlias = {
          aliasId: this.data.counters.alias++,
          entityId: entity.entityId,
          aliasText: options.alias,
          confidence: options.confidence ?? 1,
        };
        this.data.aliases.push(aliasRecord);
      }
    }

    this.dirty = true;
    await this.persist();
    return entity;
  }

  async upsertFact(input: FactWriteInput): Promise<StoredFact> {
    const validFrom = normaliseTimestamp(input.validFrom ?? new Date());
    const validTo = input.validTo ? normaliseTimestamp(input.validTo) : null;
    const objectEntityId = input.objectEntityId ?? null;
    const existing = this.data.facts.find(
      (fact) =>
        fact.subjectEntityId === input.subjectEntityId &&
        fact.predicateKey === input.predicateKey &&
        fact.objectEntityId === objectEntityId &&
        JSON.stringify(fact.objectValue) === JSON.stringify(input.objectValue) &&
        fact.validTo === null,
    );

    if (existing) {
      existing.validFrom = validFrom < existing.validFrom ? validFrom : existing.validFrom;
      existing.confidence = input.confidence ?? existing.confidence;
      if (validTo && (!existing.validTo || validTo < existing.validTo)) {
        existing.validTo = validTo;
      }
      this.dirty = true;
      await this.persist();
      return existing;
    }

    const factId = this.data.counters.fact++;
    const fact: StoredFact = {
      factId,
      subjectEntityId: input.subjectEntityId,
      predicateKey: input.predicateKey,
      objectEntityId,
      objectValue: input.objectValue ?? null,
      validFrom,
      validTo,
      confidence: input.confidence ?? 1,
      sourceEpisodeId: input.sourceEpisodeId,
    };
    this.data.facts.push(fact);
    this.dirty = true;
    await this.persist();
    return fact;
  }

  async linkEpisode(
    episodeId: number,
    options: { entityId?: number | null; factId?: number | null; role: string },
  ): Promise<EpisodeLinkRecord> {
    const targetEntity = options.entityId ?? null;
    const targetFact = options.factId ?? null;
    const existing = this.data.links.find(
      (link) =>
        link.episodeId === episodeId &&
        link.entityId === targetEntity &&
        link.factId === targetFact &&
        link.role === options.role,
    );
    if (existing) return existing;

    const link: EpisodeLinkRecord = {
      linkId: this.data.counters.link++,
      episodeId,
      entityId: targetEntity,
      factId: targetFact,
      role: options.role,
    };
    this.data.links.push(link);
    this.dirty = true;
    await this.persist();
    return link;
  }

  queryTimeline(query: TimelineQuery): StoredFact[] {
    const asOf = query.asOf ? normaliseTimestamp(query.asOf) : null;
    const betweenStart = query.between ? normaliseTimestamp(query.between[0]) : null;
    const betweenEnd = query.between ? normaliseTimestamp(query.between[1]) : null;
    const role = query.role ?? 'subject';

    return this.data.facts.filter((fact) => {
      const effectiveValidTo = fact.validTo ?? INFINITY_TS;
      const entityMatch =
        role === 'object'
          ? fact.objectEntityId === query.entityId
          : fact.subjectEntityId === query.entityId;
      if (!entityMatch) return false;
      if (query.predicateKey && fact.predicateKey !== query.predicateKey) return false;
      if (asOf) {
        return fact.validFrom <= asOf && asOf < effectiveValidTo;
      }
      if (betweenStart && betweenEnd) {
        return fact.validFrom < betweenEnd && effectiveValidTo > betweenStart;
      }
      return true;
    });
  }

  traceBack(factId: number): StoredEpisode[] {
    const relatedLinks = this.data.links.filter((link) => link.factId === factId);
    if (relatedLinks.length === 0) return [];
    const ids = new Set<number>(relatedLinks.map((link) => link.episodeId));
    return this.data.episodes.filter((episode) => ids.has(episode.episodeId));
  }

  private computeTraceHash(input: EpisodeInput, occurredAt: string): string {
    const hash = createHash('sha256');
    hash.update(input.sourceType);
    hash.update(occurredAt);
    hash.update(JSON.stringify(input.payload));
    return hash.digest('hex');
  }

  private async persist(): Promise<void> {
    const serialised = JSON.stringify(this.data, null, 2);
    await fsp.writeFile(this.filePath, serialised, 'utf8');
    this.dirty = false;
  }
}
