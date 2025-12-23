/**
 * PersistentStore - Rust-Native Storage Wrapper (v2.0)
 *
 * This is a thin TypeScript wrapper around the Rust core storage engine.
 * All storage operations are delegated to the native Rust implementation.
 */

import { openNativeHandle, nativeTemporalSupported } from '../../native/core.js';
import { LabelManager } from '../../graph/labels.js';
import type { FactInput } from './types.js';
import {
  TemporalMemoryStore,
  type EpisodeInput,
  type EpisodeLinkRecord,
  type FactWriteInput,
  type EnsureEntityOptions,
  type StoredEpisode,
  type StoredEntity,
  type StoredFact,
  type TimelineQuery,
} from './temporal/temporalStore.js';

// ============================================================================
// Types
// ============================================================================

interface NativeTriple {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

interface NativeFact extends NativeTriple {
  subject: string;
  predicate: string;
  object: string;
}

interface NativeQueryCriteria {
  subjectId?: number;
  predicateId?: number;
  objectId?: number;
}

// Graph Algorithm Types
interface NativePathResult {
  path: bigint[];
  cost: number;
  hops: number;
}

interface NativePageRankEntry {
  nodeId: bigint;
  score: number;
}

interface NativePageRankResult {
  scores: NativePageRankEntry[];
  iterations: number;
  converged: boolean;
}

interface NativeCypherRelationship {
  subjectId: bigint;
  predicateId: bigint;
  objectId: bigint;
}

export interface NativeCypherStatement {
  step(): boolean;
  columnCount(): number;
  columnName(column: number): string | null;
  columnType(column: number): number;
  columnText(column: number): string | null;
  columnFloat(column: number): number | null;
  columnBool(column: number): boolean | null;
  columnNodeId(column: number): bigint | null;
  columnRelationship(column: number): NativeCypherRelationship | null;
  finalize(): void;
}

export interface NativeDatabaseHandle {
  addFact(subject: string, predicate: string, object: string): NativeTriple;
  deleteFact(subject: string, predicate: string, object: string): boolean;
  intern(value: string): number;
  getDictionarySize(): number;
  resolveId(value: string): number | null | undefined;
  resolveStr(id: number): string | null | undefined;
  executeQuery(query: string, params?: Record<string, unknown> | null): Record<string, any>[];
  prepareV2(query: string, params?: Record<string, unknown> | null): NativeCypherStatement;
  query(criteria?: NativeQueryCriteria): NativeTriple[];
  queryFacts?(criteria?: NativeQueryCriteria): NativeFact[];
  openCursor(criteria?: NativeQueryCriteria): { id: number };
  readCursor(cursorId: number, batchSize: number): { triples: NativeTriple[]; done: boolean };
  readCursorFacts?(
    cursorId: number,
    batchSize: number,
  ): { facts: NativeFact[]; done: boolean };
  closeCursor(cursorId: number): void;
  hydrate(dictionary: string[], triples: NativeTriple[]): void;
  setNodeProperty(nodeId: number, json: string): void;
  setNodePropertyDirect?(nodeId: number, properties: Record<string, unknown>): void;
  getNodeProperty(nodeId: number): string | null | undefined;
  getNodePropertyDirect?(nodeId: number): Record<string, unknown> | null | undefined;
  setEdgeProperty(subjectId: number, predicateId: number, objectId: number, json: string): void;
  setEdgePropertyDirect?(
    subjectId: number,
    predicateId: number,
    objectId: number,
    properties: Record<string, unknown>,
  ): void;
  getEdgeProperty(
    subjectId: number,
    predicateId: number,
    objectId: number,
  ): string | null | undefined;
  getEdgePropertyDirect?(
    subjectId: number,
    predicateId: number,
    objectId: number,
  ): Record<string, unknown> | null | undefined;
  beginTransaction(): void;
  commitTransaction(): void;
  abortTransaction(): void;
  close(): void;

  // Graph Algorithms (Rust Native - optional)
  bfsShortestPath?(
    startId: bigint,
    endId: bigint,
    predicateId?: bigint | null,
    maxHops?: number | null,
    bidirectional?: boolean | null,
  ): NativePathResult | null;

  dijkstraShortestPath?(
    startId: bigint,
    endId: bigint,
    predicateId?: bigint | null,
    maxHops?: number | null,
  ): NativePathResult | null;

  pagerank?(
    predicateId?: bigint | null,
    damping?: number | null,
    maxIterations?: number | null,
    tolerance?: number | null,
  ): NativePageRankResult;
}

export interface PersistedFact extends FactInput {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

export interface FactRecord extends PersistedFact {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

// Legacy type aliases
export type TripleKey = { subjectId: number; predicateId: number; objectId: number };
export type EncodedTriple = { subjectId: number; predicateId: number; objectId: number };

export interface PersistentStoreOptions {
  // Native-only mode - JS engine removed in v2.0
  enableLock?: boolean;
  registerReader?: boolean;
}

export type { FactInput } from './types.js';
export type {
  EpisodeInput as TemporalEpisodeInput,
  EnsureEntityOptions as TemporalEnsureEntityOptions,
  FactWriteInput as TemporalFactWriteInput,
  EpisodeLinkRecord as TemporalEpisodeLinkRecord,
  StoredEpisode as TemporalStoredEpisode,
  StoredEntity as TemporalStoredEntity,
  StoredFact as TemporalStoredFact,
  TimelineQuery as TemporalTimelineQuery,
} from './temporal/temporalStore.js';

// ============================================================================
// PersistentStore - Rust Native Wrapper
// ============================================================================

export class PersistentStore {
  private constructor(
    private readonly path: string,
    private readonly native: NativeDatabaseHandle,
  ) {}

  private closed = false;
  private labelManager: LabelManager | null = null;
  private temporalStore: TemporalMemoryStore | null = null;

  /**
   * Open a database at the specified path.
   * In v2.0, this always uses the Rust native storage engine.
   */
  static async open(path: string, options: PersistentStoreOptions = {}): Promise<PersistentStore> {
    // Open native handle - throws if not available
    const native = openNativeHandle(path);

    if (!native) {
      throw new Error(
        'NervusDB v2.0 requires Rust native addon. ' +
          'Please ensure @nervusdb/core-native is installed correctly.',
      );
    }

    void options;

    const store = new PersistentStore(path, native);
    if (nativeTemporalSupported(native)) {
      await store.openTemporal();
    }
    return store;
  }

  // ========================================================================
  // Core Triple Operations
  // ========================================================================

  addFact(fact: FactInput): PersistedFact {
    this.ensureOpen();
    const triple = this.native.addFact(fact.subject, fact.predicate, fact.object);
    return {
      subject: fact.subject,
      predicate: fact.predicate,
      object: fact.object,
      subjectId: triple.subjectId,
      predicateId: triple.predicateId,
      objectId: triple.objectId,
    };
  }

  deleteFact(fact: FactInput): void {
    this.ensureOpen();
    this.native.deleteFact(fact.subject, fact.predicate, fact.object);
  }

  listFacts(): FactRecord[] {
    this.ensureOpen();
    if (typeof this.native.queryFacts === 'function') {
      return this.native.queryFacts().map((t) => this.toFactRecordResolved(t));
    }
    const triples = this.native.query();
    return triples.map((t) => this.toFactRecord(t));
  }

  // Query with string criteria
  query(criteria: Partial<{ subject: string; predicate: string; object: string }>): FactRecord[];
  // Query with encoded triple (for internal use)
  query(criteria: Partial<EncodedTriple>): FactRecord[];
  query(criteria: any): FactRecord[] {
    this.ensureOpen();

    const nativeCriteria: NativeQueryCriteria = {};

    // Handle both string and numeric criteria
    if (criteria.subject !== undefined) {
      nativeCriteria.subjectId = this.native.resolveId(criteria.subject) ?? undefined;
    } else if (criteria.subjectId !== undefined) {
      nativeCriteria.subjectId = criteria.subjectId;
    }

    if (criteria.predicate !== undefined) {
      nativeCriteria.predicateId = this.native.resolveId(criteria.predicate) ?? undefined;
    } else if (criteria.predicateId !== undefined) {
      nativeCriteria.predicateId = criteria.predicateId;
    }

    if (criteria.object !== undefined) {
      nativeCriteria.objectId = this.native.resolveId(criteria.object) ?? undefined;
    } else if (criteria.objectId !== undefined) {
      nativeCriteria.objectId = criteria.objectId;
    }

    if (typeof this.native.queryFacts === 'function') {
      const facts = this.native.queryFacts(nativeCriteria);
      return facts.map((t) => this.toFactRecordResolved(t));
    }

    const triples = this.native.query(nativeCriteria);
    return triples.map((t) => this.toFactRecord(t));
  }

  executeQuery(cypher: string, params?: Record<string, unknown>): any[] {
    this.ensureOpen();
    return this.native.executeQuery(cypher, params ?? null);
  }

  prepareV2(cypher: string, params?: Record<string, unknown>): NativeCypherStatement {
    this.ensureOpen();
    return this.native.prepareV2(cypher, params ?? null);
  }

  // ========================================================================
  // Property Operations
  // ========================================================================

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    this.ensureOpen();
    // v1.1: Use direct method if available (bypasses JSON serialization)
    if (this.native.setNodePropertyDirect) {
      this.native.setNodePropertyDirect(nodeId, properties);
    } else {
      this.native.setNodeProperty(nodeId, JSON.stringify(properties));
    }
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | undefined {
    this.ensureOpen();
    // v1.1: Use direct method if available (bypasses JSON parsing)
    if (this.native.getNodePropertyDirect) {
      return this.native.getNodePropertyDirect(nodeId) ?? undefined;
    }
    const json = this.native.getNodeProperty(nodeId);
    return json ? JSON.parse(json) : undefined;
  }

  // Edge properties (with TripleKey overload for compatibility)
  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void;
  setEdgeProperties(
    subjectId: number,
    predicateId: number,
    objectId: number,
    properties: Record<string, unknown>,
  ): void;
  setEdgeProperties(
    keyOrSubjectId: TripleKey | number,
    propertiesOrPredicateId: Record<string, unknown> | number,
    objectId?: number,
    properties?: Record<string, unknown>,
  ): void {
    this.ensureOpen();
    let s: number, p: number, o: number, props: Record<string, unknown>;

    if (typeof keyOrSubjectId === 'object') {
      const key = keyOrSubjectId as TripleKey;
      s = key.subjectId;
      p = key.predicateId;
      o = key.objectId;
      props = propertiesOrPredicateId as Record<string, unknown>;
    } else {
      s = keyOrSubjectId;
      p = propertiesOrPredicateId as number;
      o = objectId!;
      props = properties!;
    }

    // v1.1: Use direct method if available (bypasses JSON serialization)
    if (this.native.setEdgePropertyDirect) {
      this.native.setEdgePropertyDirect(s, p, o, props);
    } else {
      this.native.setEdgeProperty(s, p, o, JSON.stringify(props));
    }
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined;
  getEdgeProperties(
    subjectId: number,
    predicateId: number,
    objectId: number,
  ): Record<string, unknown> | undefined;
  getEdgeProperties(
    keyOrSubjectId: TripleKey | number,
    predicateId?: number,
    objectId?: number,
  ): Record<string, unknown> | undefined {
    this.ensureOpen();
    let s: number, p: number, o: number;

    if (typeof keyOrSubjectId === 'object') {
      const key = keyOrSubjectId as TripleKey;
      s = key.subjectId;
      p = key.predicateId;
      o = key.objectId;
    } else {
      s = keyOrSubjectId;
      p = predicateId!;
      o = objectId!;
    }

    // v1.1: Use direct method if available (bypasses JSON parsing)
    if (this.native.getEdgePropertyDirect) {
      return this.native.getEdgePropertyDirect(s, p, o) ?? undefined;
    }
    const json = this.native.getEdgeProperty(s, p, o);
    return json ? JSON.parse(json) : undefined;
  }

  // ========================================================================
  // Dictionary Operations
  // ========================================================================

  intern(value: string): number {
    this.ensureOpen();
    return this.native.intern(value);
  }

  resolveId(value: string): number | undefined {
    this.ensureOpen();
    return this.native.resolveId(value) ?? undefined;
  }

  resolveStr(id: number): string | undefined {
    this.ensureOpen();
    return this.native.resolveStr(id) ?? undefined;
  }

  getDictionarySize(): number {
    this.ensureOpen();
    return this.native.getDictionarySize();
  }

  // ========================================================================
  // Transaction Operations
  // ========================================================================

  beginTransaction(): void {
    this.ensureOpen();
    this.native.beginTransaction();
  }

  commitTransaction(): void {
    this.ensureOpen();
    this.native.commitTransaction();
  }

  abortTransaction(): void {
    this.ensureOpen();
    this.native.abortTransaction();
  }

  // ========================================================================
  // Streaming Query API
  // ========================================================================

  async *streamQuery(
    criteria: Partial<{ subject: string; predicate: string; object: string }>,
    batchSize = 1000,
  ): AsyncGenerator<FactRecord[]> {
    this.ensureOpen();

    const nativeCriteria: NativeQueryCriteria = {};
    if (criteria.subject)
      nativeCriteria.subjectId = this.native.resolveId(criteria.subject) ?? undefined;
    if (criteria.predicate)
      nativeCriteria.predicateId = this.native.resolveId(criteria.predicate) ?? undefined;
    if (criteria.object)
      nativeCriteria.objectId = this.native.resolveId(criteria.object) ?? undefined;

    const cursor = this.native.openCursor(nativeCriteria);

    try {
      while (true) {
        if (typeof this.native.readCursorFacts === 'function') {
          const { facts, done } = this.native.readCursorFacts(cursor.id, batchSize);
          if (facts.length > 0) {
            yield facts.map((t) => this.toFactRecordResolved(t));
          }
          if (done) break;
          continue;
        }

        const { triples, done } = this.native.readCursor(cursor.id, batchSize);
        if (triples.length > 0) {
          yield triples.map((t) => this.toFactRecord(t));
        }
        if (done) break;
      }
    } finally {
      this.native.closeCursor(cursor.id);
    }
  }

  // ========================================================================
  // Label Manager Integration
  // ========================================================================

  get labels(): LabelManager {
    if (!this.labelManager) {
      // Use database path as index directory for compatibility
      this.labelManager = new LabelManager(this.path);
    }
    return this.labelManager;
  }

  // ========================================================================
  // Temporal Memory Integration
  // ========================================================================

  async openTemporal(): Promise<void> {
    if (!this.temporalStore) {
      this.temporalStore = await TemporalMemoryStore.initialize(this.path, this.native);
    }
  }

  // ========================================================================
  // Legacy API Compatibility (v1.x -> v2.0)
  // ========================================================================

  // Batch operations (mapped to transactions)
  beginBatch(_options?: any): void {
    this.beginTransaction();
  }

  commitBatch(_options?: any): void {
    this.commitTransaction();
  }

  abortBatch(): void {
    this.abortTransaction();
  }

  // Epoch pinning (no-op in v2.0 - Rust handles concurrency)
  getCurrentEpoch(): number {
    return 0; // Rust manages epochs internally
  }

  async pushPinnedEpoch(_epoch: number): Promise<void> {
    // No-op: Rust redb handles reader snapshots automatically
  }

  async popPinnedEpoch(): Promise<void> {
    // No-op: Rust redb handles reader snapshots automatically
  }

  // Index metadata (no-op in v2.0 - no paged indexes)
  hasPagedIndexData(_order?: string): boolean {
    return false; // v2.0 uses Rust's internal indexing
  }

  getIndexManifest(): any {
    return { version: 2, indexes: [] }; // Rust manages indexes internally
  }

  getLabelIndex(): any {
    return new Map(); // v2.0: Rust manages label indexes
  }

  getPropertyIndex(): any {
    return new Map(); // v2.0: Rust manages property indexes
  }

  // Hotness tracking (no-op in v2.0 - Rust manages statistics)
  getHotnessSnapshot(): { counts?: any } | undefined {
    return undefined; // Rust manages hotness internally
  }

  // Staging metrics (no-op in v2.0)
  getStagingMetrics(): any {
    return { adds: 0, dels: 0, nodeProps: 0, edgeProps: 0 };
  }

  // Resolve encoded triples to fact records
  resolveRecords(triples: EncodedTriple[], _options?: any): FactRecord[] {
    // Options ignored in v2.0 (properties always included from Rust)
    return triples.map((t) => this.toFactRecord(t));
  }

  // Stream query alias (for compatibility)
  streamFactRecords(
    criteria: Partial<EncodedTriple>,
    batchSize?: number,
  ): AsyncGenerator<FactRecord[]> {
    const stringCriteria: Partial<{ subject: string; predicate: string; object: string }> = {};
    if (criteria.subjectId !== undefined) {
      stringCriteria.subject = this.resolveStr(criteria.subjectId) ?? '';
    }
    if (criteria.predicateId !== undefined) {
      stringCriteria.predicate = this.resolveStr(criteria.predicateId) ?? '';
    }
    if (criteria.objectId !== undefined) {
      stringCriteria.object = this.resolveStr(criteria.objectId) ?? '';
    }
    return this.streamQuery(stringCriteria, batchSize);
  }

  // Temporal memory methods (delegate to this.temporal)
  getTemporalMemory(): TemporalMemoryStore | undefined {
    return this.temporalStore ?? undefined;
  }

  addEpisodeToTemporalStore(episode: EpisodeInput) {
    return this.requireTemporalStore().addEpisode(episode);
  }

  ensureTemporalEntity(kind: string, canonicalName: string, options?: EnsureEntityOptions) {
    return this.requireTemporalStore().ensureEntity(kind, canonicalName, options ?? {});
  }

  upsertTemporalFact(fact: FactWriteInput) {
    return this.requireTemporalStore().upsertFact(fact);
  }

  linkTemporalEpisode(
    episodeId: number,
    linkOptions: { entityId?: number | null; factId?: number | null; role: string },
  ) {
    return this.requireTemporalStore().linkEpisode(episodeId, linkOptions);
  }

  queryTemporalTimeline(query: TimelineQuery) {
    return this.requireTemporalStore().queryTimeline(query);
  }

  traceTemporalFact(factId: number) {
    return this.requireTemporalStore().traceBack(factId);
  }

  // ========================================================================
  // Utilities & Lifecycle
  // ========================================================================

  getNodeIdByValue(value: string): number | undefined {
    return this.resolveId(value);
  }

  getNodeValueById(id: number): string | undefined {
    return this.resolveStr(id);
  }

  async flush(): Promise<void> {
    // No-op: Rust handles persistence automatically
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    if (this.temporalStore) {
      await this.temporalStore.close();
    }
    this.native.close();
  }

  /**
   * 获取底层 Native 句柄（用于直接调用 Rust 算法）
   */
  getNativeHandle(): NativeDatabaseHandle {
    this.ensureOpen();
    return this.native;
  }

  // ========================================================================
  // Private Helpers
  // ========================================================================

  private ensureOpen(): void {
    if (this.closed) {
      throw new Error('Database is closed');
    }
  }

  private requireTemporalStore(): TemporalMemoryStore {
    if (this.temporalStore) return this.temporalStore;
    throw new Error(
      'Temporal feature is disabled. Rebuild native addon with --features temporal.',
    );
  }

  private toFactRecord(triple: NativeTriple): FactRecord {
    const subject = this.native.resolveStr(triple.subjectId);
    const predicate = this.native.resolveStr(triple.predicateId);
    const object = this.native.resolveStr(triple.objectId);

    if (!subject || !predicate || !object) {
      throw new Error(
        `Failed to resolve triple: s=${String(triple.subjectId)} p=${String(triple.predicateId)} o=${String(triple.objectId)}`,
      );
    }

    return {
      subject,
      predicate,
      object,
      subjectId: triple.subjectId,
      predicateId: triple.predicateId,
      objectId: triple.objectId,
    };
  }

  private toFactRecordResolved(fact: NativeFact): FactRecord {
    if (!fact.subject || !fact.predicate || !fact.object) {
      throw new Error(
        `Failed to resolve triple: s=${String(fact.subjectId)} p=${String(fact.predicateId)} o=${String(fact.objectId)}`,
      );
    }

    return {
      subject: fact.subject,
      predicate: fact.predicate,
      object: fact.object,
      subjectId: fact.subjectId,
      predicateId: fact.predicateId,
      objectId: fact.objectId,
    };
  }
}
