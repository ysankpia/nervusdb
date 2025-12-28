import { createHash } from 'node:crypto';
import { promises as fsp } from 'node:fs';
import { dirname } from 'node:path';

const DATA_SUFFIX = '.temporal.json';
const INFINITY_TS = '9999-12-31T23:59:59.999Z';

type NativeTimelineQueryInput = {
  entity_id: string;
  predicate_key?: string;
  role?: string;
  as_of?: string;
  between_start?: string;
  between_end?: string;
};

interface NativeTimelineFactOutput {
  fact_id: string;
  subject_entity_id: string;
  predicate_key: string;
  object_entity_id?: string | null;
  object_value?: string | null;
  valid_from: string;
  valid_to?: string | null;
  confidence: number;
  source_episode_id: string;
}

interface NativeTimelineEpisodeOutput {
  episode_id: string;
  source_type: string;
  payload: string;
  occurred_at: string;
  ingested_at: string;
  trace_hash: string;
}

interface NativeTemporalEpisodeInput {
  source_type: string;
  payload_json: string;
  occurred_at: string;
  trace_hash?: string | null;
}

type NativeTemporalEpisodeOutput = NativeTimelineEpisodeOutput;

interface NativeTemporalEnsureEntityInput {
  kind: string;
  canonical_name: string;
  alias?: string | null;
  confidence?: number | null;
  occurred_at?: string | null;
  version_increment?: boolean | null;
}

interface NativeTemporalEntityOutput {
  entity_id: string;
  kind: string;
  canonical_name: string;
  fingerprint: string;
  first_seen: string;
  last_seen: string;
  version: string;
}

interface NativeTemporalFactInput {
  subject_entity_id: string;
  predicate_key: string;
  object_entity_id?: string | null;
  object_value_json?: string | null;
  valid_from?: string | null;
  valid_to?: string | null;
  confidence?: number | null;
  source_episode_id: string;
}

type NativeTemporalFactOutput = NativeTimelineFactOutput;

interface NativeTemporalLinkInput {
  episode_id: string;
  entity_id?: string | null;
  fact_id?: string | null;
  role: string;
}

interface NativeTemporalLinkOutput {
  link_id: string;
  episode_id: string;
  entity_id?: string | null;
  fact_id?: string | null;
  role: string;
}

interface NativeTemporalHandle {
  temporalListEpisodes(): NativeTemporalEpisodeOutput[];
  temporalListEntities(): NativeTemporalEntityOutput[];
  temporalListFacts(): NativeTemporalFactOutput[];
  temporalAddEpisode(input: NativeTemporalEpisodeInput): NativeTemporalEpisodeOutput;
  temporalEnsureEntity(input: NativeTemporalEnsureEntityInput): NativeTemporalEntityOutput;
  temporalUpsertFact(input: NativeTemporalFactInput): NativeTemporalFactOutput;
  temporalLinkEpisode(input: NativeTemporalLinkInput): NativeTemporalLinkOutput;
  timelineQuery(input: NativeTimelineQueryInput): NativeTimelineFactOutput[];
  timelineTrace(factId: string): NativeTimelineEpisodeOutput[];
}

function isNativeTemporalHandle(value: unknown): value is NativeTemporalHandle {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Partial<NativeTemporalHandle>;
  return (
    typeof candidate.temporalListEpisodes === 'function' &&
    typeof candidate.temporalListEntities === 'function' &&
    typeof candidate.temporalListFacts === 'function' &&
    typeof candidate.temporalAddEpisode === 'function' &&
    typeof candidate.temporalEnsureEntity === 'function' &&
    typeof candidate.temporalUpsertFact === 'function' &&
    typeof candidate.temporalLinkEpisode === 'function' &&
    typeof candidate.timelineQuery === 'function' &&
    typeof candidate.timelineTrace === 'function'
  );
}

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

interface TemporalBackend {
  close(): Promise<void>;
  getEpisodes(): StoredEpisode[];
  getEntities(): StoredEntity[];
  getFacts(): StoredFact[];
  addEpisode(input: EpisodeInput): Promise<StoredEpisode>;
  ensureEntity(
    kind: string,
    canonicalName: string,
    options: EnsureEntityOptions,
  ): Promise<StoredEntity>;
  upsertFact(input: FactWriteInput): Promise<StoredFact>;
  linkEpisode(
    episodeId: number,
    options: { entityId?: number | null; factId?: number | null; role: string },
  ): Promise<EpisodeLinkRecord>;
  queryTimeline(query: TimelineQuery): StoredFact[];
  traceBack(factId: number): StoredEpisode[];
}

export interface EnsureEntityOptions {
  alias?: string;
  confidence?: number;
  occurredAt?: TimestampInput;
  versionIncrement?: boolean;
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

class JsonTemporalBackend implements TemporalBackend {
  static async create(dataPath: string): Promise<JsonTemporalBackend> {
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

    return new JsonTemporalBackend(filePath, payload);
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

    const ensured = entity;
    if (!ensured) {
      throw new Error('Temporal entity was not initialised');
    }

    if (options.alias) {
      const aliasLower = options.alias.toLowerCase();
      const existingAlias = this.data.aliases.find((alias) => {
        return alias.entityId === ensured.entityId && alias.aliasText.toLowerCase() === aliasLower;
      });
      if (!existingAlias) {
        const aliasRecord: StoredAlias = {
          aliasId: this.data.counters.alias++,
          entityId: ensured.entityId,
          aliasText: options.alias,
          confidence: options.confidence ?? 1,
        };
        this.data.aliases.push(aliasRecord);
      }
    }

    this.dirty = true;
    await this.persist();
    return ensured;
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

class NativeTemporalBackend implements TemporalBackend {
  constructor(private readonly handle: NativeTemporalHandle) {}

  close(): Promise<void> {
    // Native handle lifecycle is managed by PersistentStore; no additional flush required here.
    return Promise.resolve();
  }

  getEpisodes(): StoredEpisode[] {
    return this.handle.temporalListEpisodes().map((episode) => fromNativeEpisode(episode));
  }

  getEntities(): StoredEntity[] {
    return this.handle.temporalListEntities().map((entity) => fromNativeEntity(entity));
  }

  getFacts(): StoredFact[] {
    return this.handle.temporalListFacts().map((fact) => fromNativeFact(fact));
  }

  addEpisode(input: EpisodeInput): Promise<StoredEpisode> {
    const nativeInput: NativeTemporalEpisodeInput = {
      source_type: input.sourceType,
      payload_json: JSON.stringify(input.payload ?? null),
      occurred_at: normaliseTimestamp(input.occurredAt),
      trace_hash: input.traceHash ?? null,
    };
    const result = this.handle.temporalAddEpisode(nativeInput);
    return Promise.resolve(fromNativeEpisode(result));
  }

  ensureEntity(
    kind: string,
    canonicalName: string,
    options: EnsureEntityOptions,
  ): Promise<StoredEntity> {
    const nativeInput: NativeTemporalEnsureEntityInput = {
      kind,
      canonical_name: canonicalName,
      alias: options.alias ?? null,
      confidence: options.confidence ?? null,
      occurred_at: options.occurredAt ? normaliseTimestamp(options.occurredAt) : null,
      version_increment: options.versionIncrement ?? null,
    };
    const result = this.handle.temporalEnsureEntity(nativeInput);
    return Promise.resolve(fromNativeEntity(result));
  }

  upsertFact(input: FactWriteInput): Promise<StoredFact> {
    const nativeInput: NativeTemporalFactInput = {
      subject_entity_id: input.subjectEntityId.toString(),
      predicate_key: input.predicateKey,
      object_entity_id: input.objectEntityId != null ? input.objectEntityId.toString() : null,
      object_value_json:
        input.objectValue === undefined ? null : JSON.stringify(input.objectValue ?? null),
      valid_from: input.validFrom ? normaliseTimestamp(input.validFrom) : null,
      valid_to: input.validTo ? normaliseTimestamp(input.validTo) : null,
      confidence: input.confidence ?? null,
      source_episode_id: input.sourceEpisodeId.toString(),
    };
    const result = this.handle.temporalUpsertFact(nativeInput);
    return Promise.resolve(fromNativeFact(result));
  }

  linkEpisode(
    episodeId: number,
    options: { entityId?: number | null; factId?: number | null; role: string },
  ): Promise<EpisodeLinkRecord> {
    const nativeInput: NativeTemporalLinkInput = {
      episode_id: episodeId.toString(),
      entity_id: options.entityId != null ? options.entityId.toString() : null,
      fact_id: options.factId != null ? options.factId.toString() : null,
      role: options.role,
    };
    const result = this.handle.temporalLinkEpisode(nativeInput);
    return Promise.resolve(fromNativeLink(result));
  }

  queryTimeline(query: TimelineQuery): StoredFact[] {
    const nativeQuery: NativeTimelineQueryInput = {
      entity_id: query.entityId.toString(),
      predicate_key: query.predicateKey,
      role: query.role,
      as_of: query.asOf ? normaliseTimestamp(query.asOf) : undefined,
      between_start: query.between ? normaliseTimestamp(query.between[0]) : undefined,
      between_end: query.between ? normaliseTimestamp(query.between[1]) : undefined,
    };
    return this.handle.timelineQuery(nativeQuery).map((fact) => fromNativeFact(fact));
  }

  traceBack(factId: number): StoredEpisode[] {
    return this.handle
      .timelineTrace(factId.toString())
      .map((episode) => fromNativeEpisode(episode));
  }
}

export class TemporalMemoryStore {
  private constructor(private readonly backend: TemporalBackend) {}

  static async initialize(
    _dataPath: string,
    nativeHandle?: unknown,
  ): Promise<TemporalMemoryStore> {
    if (isNativeTemporalHandle(nativeHandle)) {
      return new TemporalMemoryStore(new NativeTemporalBackend(nativeHandle));
    }
    throw new Error(
      'Temporal feature is disabled. Rebuild native addon with --features temporal.',
    );
  }

  async close(): Promise<void> {
    await this.backend.close();
  }

  getEpisodes(): StoredEpisode[] {
    return this.backend.getEpisodes();
  }

  getEntities(): StoredEntity[] {
    return this.backend.getEntities();
  }

  getFacts(): StoredFact[] {
    return this.backend.getFacts();
  }

  async addEpisode(input: EpisodeInput): Promise<StoredEpisode> {
    return this.backend.addEpisode(input);
  }

  async ensureEntity(
    kind: string,
    canonicalName: string,
    options: EnsureEntityOptions = {},
  ): Promise<StoredEntity> {
    return this.backend.ensureEntity(kind, canonicalName, options);
  }

  async upsertFact(input: FactWriteInput): Promise<StoredFact> {
    return this.backend.upsertFact(input);
  }

  async linkEpisode(
    episodeId: number,
    options: { entityId?: number | null; factId?: number | null; role: string },
  ): Promise<EpisodeLinkRecord> {
    return this.backend.linkEpisode(episodeId, options);
  }

  queryTimeline(query: TimelineQuery): StoredFact[] {
    return this.backend.queryTimeline(query);
  }

  traceBack(factId: number): StoredEpisode[] {
    return this.backend.traceBack(factId);
  }
}

function fromNativeEpisode(episode: NativeTimelineEpisodeOutput): StoredEpisode {
  return {
    episodeId: parseNativeInt(episode.episode_id, 'episode_id'),
    sourceType: episode.source_type,
    payload: parseNativeJson(episode.payload, 'payload'),
    occurredAt: episode.occurred_at,
    ingestedAt: episode.ingested_at,
    traceHash: episode.trace_hash,
  };
}

function fromNativeEntity(entity: NativeTemporalEntityOutput): StoredEntity {
  return {
    entityId: parseNativeInt(entity.entity_id, 'entity_id'),
    kind: entity.kind,
    canonicalName: entity.canonical_name,
    fingerprint: entity.fingerprint,
    firstSeen: entity.first_seen,
    lastSeen: entity.last_seen,
    version: parseNativeInt(entity.version, 'version'),
  };
}

function fromNativeFact(fact: NativeTimelineFactOutput): StoredFact {
  return {
    factId: parseNativeInt(fact.fact_id, 'fact_id'),
    subjectEntityId: parseNativeInt(fact.subject_entity_id, 'subject_entity_id'),
    predicateKey: fact.predicate_key,
    objectEntityId:
      fact.object_entity_id == null
        ? null
        : parseNativeInt(fact.object_entity_id, 'object_entity_id'),
    objectValue:
      fact.object_value == null ? null : parseNativeJson(fact.object_value, 'object_value'),
    validFrom: fact.valid_from,
    validTo: fact.valid_to ?? null,
    confidence: fact.confidence,
    sourceEpisodeId: parseNativeInt(fact.source_episode_id, 'source_episode_id'),
  };
}

function fromNativeLink(record: NativeTemporalLinkOutput): EpisodeLinkRecord {
  return {
    linkId: parseNativeInt(record.link_id, 'link_id'),
    episodeId: parseNativeInt(record.episode_id, 'episode_id'),
    entityId: record.entity_id == null ? null : parseNativeInt(record.entity_id, 'entity_id'),
    factId: record.fact_id == null ? null : parseNativeInt(record.fact_id, 'fact_id'),
    role: record.role,
  };
}

function parseNativeInt(value: string, field: string): number {
  const parsed = Number.parseInt(value, 10);
  if (Number.isNaN(parsed)) {
    throw new Error(`Invalid numeric value for ${field}: ${value}`);
  }
  return parsed;
}

function parseNativeJson(raw: string, field: string): unknown {
  try {
    return JSON.parse(raw);
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to parse JSON for ${field}: ${reason}`);
  }
}
