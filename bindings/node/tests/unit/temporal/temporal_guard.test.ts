import { afterEach, describe, expect, it } from 'vitest';

import { __setNativeCoreForTesting } from '../../../src/native/core.js';
import { NervusDB } from '../../../src/synapseDb.js';

describe('Temporal feature guard', () => {
  afterEach(() => {
    __setNativeCoreForTesting(undefined);
  });

  it('returns undefined store and fails fast when temporal is not supported by native addon', async () => {
    __setNativeCoreForTesting({
      open: () =>
        ({
          close() {},
        }) as any,
    });

    const db = await NervusDB.open('tmp-temporal-disabled.redb');

    expect(db.memory.getStore()).toBeUndefined();
    expect(() =>
      db.memory.addEpisode({
        sourceType: 'demo',
        payload: null,
        occurredAt: Date.now(),
      }),
    ).toThrow(/Temporal feature is disabled/);

    await db.close();
  });
});

