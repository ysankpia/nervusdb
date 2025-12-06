import { describe, expect, it, beforeAll, afterEach } from 'vitest';

import { loadNativeCore, __setNativeCoreForTesting } from '../../../src/native/core.js';

describe('native binding availability', () => {
  beforeAll(() => {
    __setNativeCoreForTesting(undefined);
  });

  afterEach(() => {
    __setNativeCoreForTesting(undefined);
    delete process.env.NERVUSDB_DISABLE_NATIVE;
  });

  it('falls back gracefully when addon is missing', () => {
    process.env.NERVUSDB_DISABLE_NATIVE = '1';
    expect(loadNativeCore()).toBeNull();
    delete process.env.NERVUSDB_DISABLE_NATIVE;
  });

  it('loads native binding when matrix expects it', () => {
    const expectNative = process.env.NERVUSDB_EXPECT_NATIVE === '1';
    const binding = loadNativeCore();
    if (expectNative) {
      expect(binding).toBeTruthy();
    } else {
      expect(binding).toBeNull();
    }
  });
});
