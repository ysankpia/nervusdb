import { describe, expect, it, afterEach, vi } from 'vitest';

import {
  __setNativeCoreForTesting,
  loadNativeCore,
  nativeTemporalSupported,
  type NativeTemporalHandle,
  type NativeCoreBinding,
} from '../../../src/native/core.js';

describe('native core loader', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;
    delete process.env.NERVUSDB_EXPECT_NATIVE;
  });

  it('returns null when binding is not available', () => {
    __setNativeCoreForTesting(undefined);
    process.env.NERVUSDB_DISABLE_NATIVE = '1';
    expect(loadNativeCore()).toBeNull();
  });

  it('returns cached binding', () => {
    const binding: NativeCoreBinding = {
      open: () => ({
        addFact: () => ({ subject_id: 1, predicate_id: 2, object_id: 3 }),
        close: () => {},
      }),
    };
    __setNativeCoreForTesting(binding);
    expect(loadNativeCore()).toBe(binding);
  });

  it('can be force-disabled via env', () => {
    process.env.NERVUSDB_DISABLE_NATIVE = '1';
    expect(loadNativeCore()).toBeNull();
  });

  it('throws when native addon is explicitly required but missing', () => {
    const cwdSpy = vi.spyOn(process, 'cwd').mockReturnValue('/tmp/nervusdb-missing');
    process.env.NERVUSDB_EXPECT_NATIVE = '1';
    __setNativeCoreForTesting(undefined);
    try {
      expect(() => loadNativeCore()).toThrow();
    } finally {
      cwdSpy.mockRestore();
    }
  });
});

describe('native temporal capability guard', () => {
  const baseHandle = {
    addFact: () => ({ subject_id: 1, predicate_id: 2, object_id: 3 }),
    query: () => [],
    openCursor: () => ({ id: 1 }),
    readCursor: () => ({ triples: [], done: true }),
    closeCursor: () => {},
    hydrate: () => {},
    close: () => {},
  };

  it('returns true only when all temporal methods exist', () => {
    const fullHandle: NativeTemporalHandle = {
      ...baseHandle,
      timelineQuery: () => [],
      timelineTrace: () => [],
      temporalAddEpisode: () => ({
        episode_id: '1',
        source_type: 'demo',
        payload: '',
        occurred_at: '',
        ingested_at: '',
        trace_hash: '',
      }),
      temporalEnsureEntity: () => ({
        entity_id: '1',
        kind: 'demo',
        canonical_name: 'demo',
        fingerprint: 'demo',
        first_seen: '',
        last_seen: '',
        version: '1',
      }),
      temporalUpsertFact: () => ({
        fact_id: '1',
        subject_entity_id: 'S',
        predicate_key: 'p',
        object_entity_id: null,
        valid_from: '',
        valid_to: null,
        confidence: 1,
        source_episode_id: '1',
      }),
      temporalLinkEpisode: () => ({
        link_id: '1',
        episode_id: '1',
        role: 'author',
      }),
      temporalListEntities: () => [],
      temporalListEpisodes: () => [],
      temporalListFacts: () => [],
    };
    expect(nativeTemporalSupported(fullHandle)).toBe(true);

    const missingLink = {
      ...fullHandle,
      temporalLinkEpisode: undefined,
    } as unknown as NativeTemporalHandle;
    expect(nativeTemporalSupported(missingLink)).toBe(false);
  });
});
