import { createRequire } from 'node:module';
import { existsSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

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
  hydrate(dictionary: string[], triples: NativeTriple[]): void;
  close(): void;
}

export interface NativeCoreBinding {
  open(options: NativeOpenOptions): NativeDatabaseHandle;
}

let cachedBinding: NativeCoreBinding | null | undefined;

function resolveNativeAddonPath(): string | null {
  const baseDir = fileURLToPath(new URL('../../native/nervusdb-node', import.meta.url));
  const direct = join(baseDir, 'index.node');
  if (existsSync(direct)) return direct;

  const npmDir = join(baseDir, 'npm');
  if (existsSync(npmDir)) {
    for (const entry of readdirSync(npmDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const candidate = join(npmDir, entry.name, 'index.node');
      if (existsSync(candidate)) {
        return candidate;
      }
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
      cachedBinding = null;
    }
  } catch {
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
