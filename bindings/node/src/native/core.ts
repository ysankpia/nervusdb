import { createRequire } from 'node:module';
import { existsSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Minimal loader for the upcoming Rust native bindings.
 *
 * The actual implementation will be provided by a N-API addon.
 * Until then we expose a graceful fallback that allows the rest of the
 * TypeScript runtime to detect whether the native layer is available.
 */

export interface NativeOpenOptions {
  dataPath: string;
}

export interface NativeAddFactOutput {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

export interface NativeQueryCriteria {
  subjectId?: number;
  predicateId?: number;
  objectId?: number;
}

export type NativeTriple = NativeAddFactOutput;

export interface NativeFactOutput extends NativeTriple {
  subject: string;
  predicate: string;
  object: string;
}

export interface NativeFactCursorBatch {
  facts: NativeFactOutput[];
  done: boolean;
}

export interface NativeCypherRelationship {
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

export interface NativeTimelineQueryInput {
  entity_id: string;
  predicate_key?: string;
  role?: string;
  as_of?: string;
  between_start?: string;
  between_end?: string;
}

export interface NativeTimelineFactOutput {
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

export interface NativeTimelineEpisodeOutput {
  episode_id: string;
  source_type: string;
  payload: string;
  occurred_at: string;
  ingested_at: string;
  trace_hash: string;
}

export interface NativeTemporalEpisodeInput {
  source_type: string;
  payload_json: string;
  occurred_at: string;
  trace_hash?: string | null;
}

export interface NativeTemporalEpisodeOutput extends NativeTimelineEpisodeOutput {
  __brand_temporalEpisode?: true;
}

export interface NativeTemporalEnsureEntityInput {
  kind: string;
  canonical_name: string;
  alias?: string | null;
  confidence?: number | null;
  occurred_at?: string | null;
  version_increment?: boolean | null;
}

export interface NativeTemporalEntityOutput {
  entity_id: string;
  kind: string;
  canonical_name: string;
  fingerprint: string;
  first_seen: string;
  last_seen: string;
  version: string;
}

export interface NativeTemporalFactInput {
  subject_entity_id: string;
  predicate_key: string;
  object_entity_id?: string | null;
  object_value_json?: string | null;
  valid_from?: string | null;
  valid_to?: string | null;
  confidence?: number | null;
  source_episode_id: string;
}

export interface NativeTemporalFactOutput extends NativeTimelineFactOutput {
  __brand_temporalFact?: true;
}

export interface NativeTemporalLinkInput {
  episode_id: string;
  entity_id?: string | null;
  fact_id?: string | null;
  role: string;
}

export interface NativeTemporalLinkOutput {
  link_id: string;
  episode_id: string;
  entity_id?: string | null;
  fact_id?: string | null;
  role: string;
}

// Graph Algorithm Types
export interface NativePathResult {
  path: bigint[];
  cost: number;
  hops: number;
}

export interface NativePageRankEntry {
  nodeId: bigint;
  score: number;
}

export interface NativePageRankResult {
  scores: NativePageRankEntry[];
  iterations: number;
  converged: boolean;
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
  queryFacts?(criteria?: NativeQueryCriteria): NativeFactOutput[];
  openCursor(criteria?: NativeQueryCriteria): { id: number };
  readCursor(cursorId: number, batchSize: number): { triples: NativeTriple[]; done: boolean };
  readCursorFacts?(cursorId: number, batchSize: number): NativeFactCursorBatch;
  closeCursor(cursorId: number): void;
  hydrate(dictionary: string[], triples: NativeTriple[]): void;
  setNodeProperty(nodeId: number, json: string): void;
  getNodeProperty(nodeId: number): string | null | undefined;
  setEdgeProperty(subjectId: number, predicateId: number, objectId: number, json: string): void;
  getEdgeProperty(
    subjectId: number,
    predicateId: number,
    objectId: number,
  ): string | null | undefined;
  timelineQuery?(input: NativeTimelineQueryInput): NativeTimelineFactOutput[];
  timelineTrace?(factId: string): NativeTimelineEpisodeOutput[];
  temporalAddEpisode?(input: NativeTemporalEpisodeInput): NativeTemporalEpisodeOutput;
  temporalEnsureEntity?(input: NativeTemporalEnsureEntityInput): NativeTemporalEntityOutput;
  temporalUpsertFact?(input: NativeTemporalFactInput): NativeTemporalFactOutput;
  temporalLinkEpisode?(input: NativeTemporalLinkInput): NativeTemporalLinkOutput;
  temporalListEntities?(): NativeTemporalEntityOutput[];
  temporalListEpisodes?(): NativeTemporalEpisodeOutput[];
  temporalListFacts?(): NativeTemporalFactOutput[];
  beginTransaction(): void;
  commitTransaction(): void;
  abortTransaction(): void;
  close(): void;

  // Graph Algorithms (Rust Native)
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

export function openNativeHandle(path: string): NativeDatabaseHandle | null {
  const binding = loadNativeCore();
  if (!binding) return null;
  try {
    return binding.open({ dataPath: path });
  } catch (error) {
    if (process.env.NERVUSDB_NATIVE_STRICT === '1') {
      throw error;
    }
    return null;
  }
}

export interface NativeTemporalHandle extends NativeDatabaseHandle {
  timelineQuery(input: NativeTimelineQueryInput): NativeTimelineFactOutput[];
  timelineTrace(factId: string): NativeTimelineEpisodeOutput[];
  temporalAddEpisode(input: NativeTemporalEpisodeInput): NativeTemporalEpisodeOutput;
  temporalEnsureEntity(input: NativeTemporalEnsureEntityInput): NativeTemporalEntityOutput;
  temporalUpsertFact(input: NativeTemporalFactInput): NativeTemporalFactOutput;
  temporalLinkEpisode(input: NativeTemporalLinkInput): NativeTemporalLinkOutput;
  temporalListEntities(): NativeTemporalEntityOutput[];
  temporalListEpisodes(): NativeTemporalEpisodeOutput[];
  temporalListFacts(): NativeTemporalFactOutput[];
}

export interface NativeCoreBinding {
  open(options: NativeOpenOptions): NativeDatabaseHandle;
}

let cachedBinding: NativeCoreBinding | null | undefined;

function resolveNativeAddonPath(): string | null {
  // Resolve relative to this module's location, not cwd
  // import.meta.url example: file:///path/to/node_modules/@nervusdb/core/dist/index.mjs
  // We need to go up from dist/ to find native/
  const moduleDir = new URL('..', import.meta.url).pathname;
  const baseDirCandidates = [
    // When running from dist/, moduleDir points to package root.
    join(moduleDir, 'native', 'nervusdb-node'),
    // When running from src/, moduleDir points to <root>/src. Fall back to <root>/native.
    join(moduleDir, '..', 'native', 'nervusdb-node'),
  ];

  const packageName = 'nervusdb-node';
  const platform = process.platform;
  const arch = process.arch;
  const platformToken = `${platform}-${arch}`;

  let lastCheckedNpmDir: string | null = null;
  let lastAvailableFiles: string[] | null = null;

  for (const baseDir of baseDirCandidates) {
    const direct = join(baseDir, 'index.node');
    if (existsSync(direct)) return direct;

    const npmDir = join(baseDir, 'npm');
    if (!existsSync(npmDir)) {
      continue;
    }
    lastCheckedNpmDir = npmDir;

    const entries = readdirSync(npmDir, { withFileTypes: true });

    // Include both regular files and symlinks in our search
    const fileEntries = entries.filter(
      (entry) => (entry.isFile() || entry.isSymbolicLink()) && entry.name.endsWith('.node'),
    );
    lastAvailableFiles = fileEntries.map((entry) => entry.name);

    // 1. Prefer platform-specific outputs when present (napi --platform).
    const indexExactMatch = fileEntries.find(
      (entry) =>
        entry.name.startsWith('index.') &&
        entry.name.includes(platform) &&
        entry.name.includes(arch) &&
        entry.name.endsWith('.node'),
    );
    if (indexExactMatch) {
      return join(npmDir, indexExactMatch.name);
    }

    const indexLibcAwareMatch = fileEntries.find(
      (entry) => entry.name.startsWith('index.') && entry.name.includes(platformToken),
    );
    if (indexLibcAwareMatch) {
      return join(npmDir, indexLibcAwareMatch.name);
    }

    // 2. Fallback: index.node directly in npm directory (non --platform build output).
    const npmIndex = fileEntries.find((entry) => entry.name === 'index.node');
    if (npmIndex) {
      return join(npmDir, npmIndex.name);
    }

    const exactMatch = fileEntries.find(
      (entry) =>
        entry.name.startsWith(`${packageName}.`) &&
        entry.name.includes(platform) &&
        entry.name.includes(arch),
    );
    if (exactMatch) {
      return join(npmDir, exactMatch.name);
    }

    // Some platforms may include libc info (e.g. gnu/musl). Try partial match on platform token.
    const libcAwareMatch = fileEntries.find(
      (entry) => entry.name.startsWith(`${packageName}.`) && entry.name.includes(platformToken),
    );
    if (libcAwareMatch) {
      return join(npmDir, libcAwareMatch.name);
    }

    const indexFallback = fileEntries.find((entry) => entry.name.startsWith('index.'));
    if (indexFallback) {
      return join(npmDir, indexFallback.name);
    }

    const fallbackFile = fileEntries.find((entry) => entry.name.startsWith(`${packageName}.`));
    if (fallbackFile) {
      return join(npmDir, fallbackFile.name);
    }

    // Check platform-specific subdirectories (e.g., darwin-arm64/)
    const dirEntries = entries.filter((entry) => entry.isDirectory());

    // First pass: Look for exact platform match
    for (const dirEntry of dirEntries) {
      if (!dirEntry.name.includes(platformToken)) continue;

      const dirPath = join(npmDir, dirEntry.name);
      try {
        const dirFiles = readdirSync(dirPath, { withFileTypes: true });
        const nodeFiles = dirFiles.filter((f) => f.isFile() && f.name.endsWith('.node'));

        if (nodeFiles.length > 0) {
          const exactInDir = nodeFiles.find((f) => f.name.includes(platformToken));
          if (exactInDir) {
            return join(dirPath, exactInDir.name);
          }
          // Fallback to any .node file in matching directory
          return join(dirPath, nodeFiles[0].name);
        }
      } catch {
        continue;
      }
    }

    // Second pass: Look for legacy index.node in any subdirectory (fallback)
    // Only do this if we haven't found a specific platform match
    for (const dirEntry of dirEntries) {
      const dirPath = join(npmDir, dirEntry.name);
      try {
        const dirFiles = readdirSync(dirPath, { withFileTypes: true });
        const indexNode = dirFiles.find((f) => f.isFile() && f.name === 'index.node');
        if (indexNode) {
          return join(dirPath, indexNode.name);
        }
      } catch {
        continue;
      }
    }
  }

  if (process.env.NERVUSDB_EXPECT_NATIVE === '1') {
    if (lastCheckedNpmDir) {
      console.error(
        `[Native Loader] Failed to resolve addon. Platform=${platform}, arch=${arch}. Expecting file like "index.${platformToken}.node" or "${packageName}.${platformToken}.node" in ${lastCheckedNpmDir}. Available files: ${lastAvailableFiles?.join(', ') || 'none'}.`,
      );
    } else {
      console.error(
        `[Native Loader] Native npm directory not found in candidates: ${baseDirCandidates
          .map((d) => join(d, 'npm'))
          .join(', ')}.`,
      );
    }
  }

  return null;
}

/**
 * Loads the native binding in a resilient way. If the addon is missing
 * (e.g. during local development or on unsupported platforms) we simply
 * return `null` and let the TypeScript implementation take over.
 */
export function loadNativeCore(): NativeCoreBinding | null {
  if (cachedBinding !== undefined) {
    return cachedBinding;
  }

  if (process.env.NERVUSDB_DISABLE_NATIVE === '1') {
    cachedBinding = null;
    return cachedBinding;
  }

  try {
    const requireNative = createRequire(import.meta.url);
    const addonPath = resolveNativeAddonPath();

    if (addonPath) {
      const binding = requireNative(addonPath) as NativeCoreBinding;
      cachedBinding = binding;
    } else {
      if (process.env.NERVUSDB_EXPECT_NATIVE === '1') {
        throw new Error(`Native addon expected but not found in ${addonPath ?? 'resolved paths'}`);
      }
      cachedBinding = null;
    }
  } catch (error) {
    if (process.env.NERVUSDB_EXPECT_NATIVE === '1') {
      throw error instanceof Error ? error : new Error(String(error));
    }
    if (process.env.NERVUSDB_DEBUG_NATIVE === '1') {
      console.error('[Native Loader] Error loading native module:', error);
    }
    cachedBinding = null;
  }
  return cachedBinding;
}

/**
 * Allows tests to override the cached binding.
 */
export function __setNativeCoreForTesting(binding: NativeCoreBinding | null | undefined): void {
  if (binding === undefined) {
    cachedBinding = undefined;
    return;
  }
  cachedBinding = binding ?? null;
}

export function nativeTemporalSupported(
  handle: NativeDatabaseHandle | null | undefined,
): handle is NativeTemporalHandle {
  return Boolean(
    handle &&
      typeof handle.timelineQuery === 'function' &&
      typeof handle.timelineTrace === 'function' &&
      typeof handle.temporalAddEpisode === 'function' &&
      typeof handle.temporalEnsureEntity === 'function' &&
      typeof handle.temporalUpsertFact === 'function' &&
      typeof handle.temporalLinkEpisode === 'function' &&
      typeof handle.temporalListEntities === 'function' &&
      typeof handle.temporalListEpisodes === 'function' &&
      typeof handle.temporalListFacts === 'function',
  );
}
