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
  subject_id: number;
  predicate_id: number;
  object_id: number;
}

export interface NativeQueryCriteria {
  subject_id?: number;
  predicate_id?: number;
  object_id?: number;
}

export type NativeTriple = NativeAddFactOutput;

export interface NativeDatabaseHandle {
  addFact(subject: string, predicate: string, object: string): NativeTriple;
  query(criteria?: NativeQueryCriteria): NativeTriple[];
  openCursor(criteria?: NativeQueryCriteria): { id: number };
  readCursor(cursorId: number, batchSize: number): { triples: NativeTriple[]; done: boolean };
  closeCursor(cursorId: number): void;
  hydrate(dictionary: string[], triples: NativeTriple[]): void;
  close(): void;
}

export interface NativeCoreBinding {
  open(options: NativeOpenOptions): NativeDatabaseHandle;
}

let cachedBinding: NativeCoreBinding | null | undefined;

function resolveNativeAddonPath(): string | null {
  const baseDir = join(process.cwd(), 'native', 'nervusdb-node');
  const direct = join(baseDir, 'index.node');
  if (existsSync(direct)) return direct;

  const npmDir = join(baseDir, 'npm');
  if (!existsSync(npmDir)) {
    if (process.env.NERVUSDB_EXPECT_NATIVE === '1') {
      console.error(`[Native Loader] Native npm directory not found: ${npmDir}`);
    }
    return null;
  }

  const packageName = 'nervusdb-node';
  const platform = process.platform;
  const arch = process.arch;
  const entries = readdirSync(npmDir, { withFileTypes: true });

  const fileEntries = entries.filter((entry) => entry.isFile() && entry.name.endsWith('.node'));
  const platformToken = `${platform}-${arch}`;

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

  const fallbackFile = fileEntries.find((entry) => entry.name.startsWith(`${packageName}.`));
  if (fallbackFile) {
    return join(npmDir, fallbackFile.name);
  }

  const legacyEntry = entries.find((entry) => entry.isDirectory());
  if (legacyEntry) {
    const candidate = join(npmDir, legacyEntry.name, 'index.node');
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  if (process.env.NERVUSDB_EXPECT_NATIVE === '1') {
    const availableFiles = fileEntries.map((entry) => entry.name);
    console.error(
      `[Native Loader] Failed to resolve addon. Platform=${platform}, arch=${arch}. Expecting file like "${packageName}.${platformToken}.node" in ${npmDir}. Available files: ${availableFiles.join(', ') || 'none'}.`,
    );
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
    cachedBinding = null;
  }
  return cachedBinding;
}

/**
 * Allows tests to override the cached binding.
 */
export function __setNativeCoreForTesting(binding: NativeCoreBinding | null | undefined): void {
  cachedBinding = binding ?? null;
}
