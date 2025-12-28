/**
 * Shared storage-layer type definitions.
 *
 * These interfaces are referenced across the Rust-native persistent store and higher-level APIs.
 * Having a single definition avoids duplicate exports that break aggregate re-exporters.
 */
export interface FactInput {
  subject: string;
  predicate: string;
  object: string;
}
