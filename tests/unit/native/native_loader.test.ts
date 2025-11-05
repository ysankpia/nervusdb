import { describe, expect, it, afterEach } from 'vitest';

import {
  __setNativeCoreForTesting,
  loadNativeCore,
  type NativeCoreBinding,
} from '../../../src/native/core.js';

describe('native core loader', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;
  });

  it('returns null when binding is not available', () => {
    __setNativeCoreForTesting(undefined);
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
});
