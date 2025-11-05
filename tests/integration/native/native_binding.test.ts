import { describe, expect, it, beforeAll } from 'vitest';

import { loadNativeCore, __setNativeCoreForTesting } from '../../../src/native/core.js';

describe('native binding availability', () => {
  beforeAll(() => {
    __setNativeCoreForTesting(undefined);
  });

  it('falls back gracefully when addon is missing', () => {
    process.env.NERVUSDB_DISABLE_NATIVE = '1';
    expect(loadNativeCore()).toBeNull();
    delete process.env.NERVUSDB_DISABLE_NATIVE;
  });
});
