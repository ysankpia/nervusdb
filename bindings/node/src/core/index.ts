/**
 * NervusDB Core - Database Kernel (v2.0)
 *
 * v2.0 uses Rust-native storage engine exclusively.
 * All storage and query operations are delegated to the Rust core.
 *
 * This module exports:
 * - PersistentStore - Main database interface (wraps Rust core)
 *   - Use store.executeQuery(cypher) for all queries
 * - TemporalMemoryStore - Temporal memory features
 *
 * DEPRECATED (removed in v2.0):
 * - QueryBuilder - Use store.executeQuery(cypher) instead
 */

// Storage layer (Rust-backed)
export * from './storage/persistentStore.js';
export * from './storage/temporal/temporalStore.js';
