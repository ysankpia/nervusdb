import { describe, expect, it } from 'vitest';

import { StringDictionary } from '@/core/storage/dictionary.js';
import { TripleStore } from '@/core/storage/tripleStore.js';
import { PropertyStore } from '@/core/storage/propertyStore.js';

describe('change detection version tracking', () => {
  it('tracks dictionary mutations only for new values', () => {
    const dict = new StringDictionary(['seed']);

    expect(dict.getVersion()).toBe(0);

    dict.getOrCreateId('seed');
    expect(dict.getVersion()).toBe(0);

    dict.getOrCreateId('new-value');
    expect(dict.getVersion()).toBe(1);

    dict.getOrCreateId('new-value');
    expect(dict.getVersion()).toBe(1);
  });

  it('tracks triple additions but ignores duplicates', () => {
    const store = new TripleStore([{ subjectId: 1, predicateId: 2, objectId: 3 }]);

    expect(store.getVersion()).toBe(0);

    store.add({ subjectId: 2, predicateId: 3, objectId: 4 });
    expect(store.getVersion()).toBe(1);

    store.add({ subjectId: 2, predicateId: 3, objectId: 4 });
    expect(store.getVersion()).toBe(1);
  });

  it('bumps property store version only when serialized bytes change', () => {
    const store = new PropertyStore();

    expect(store.getVersion()).toBe(0);

    store.setNodeProperties(1, { foo: 'bar' });
    expect(store.getVersion()).toBe(1);

    // Subsequent writes bump the version, even when targeting the same node
    store.setNodeProperties(1, { foo: 'bar' });
    expect(store.getVersion()).toBe(2);

    store.setEdgeProperties({ subjectId: 1, predicateId: 2, objectId: 3 }, { weight: 1 });
    expect(store.getVersion()).toBe(3);
  });
});
