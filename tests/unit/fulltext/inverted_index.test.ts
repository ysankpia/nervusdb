import { describe, expect, it } from 'vitest';

import { MemoryInvertedIndex } from '../../../src/extensions/fulltext/invertedIndex.js';
import type { Document, Token } from '../../../src/extensions/fulltext/types.js';

function makeDoc(id: string, tokens: Token[]): Document {
  return {
    id,
    fields: new Map([['content', tokens.map((t) => t.value).join(' ')]]),
    tokens,
  };
}

describe('MemoryInvertedIndex', () => {
  it('aggregates positions per term per document', () => {
    const index = new MemoryInvertedIndex();
    const doc = makeDoc('doc-1', [
      { value: 'graph', type: 'word', position: 0, length: 5 },
      { value: 'database', type: 'word', position: 1, length: 8 },
      { value: 'graph', type: 'word', position: 2, length: 5 },
    ]);
    index.addDocument(doc);

    const posting = index.getPostingList('graph');
    expect(posting).toBeDefined();
    expect(posting?.entries).toHaveLength(1);
    const entry = posting!.entries[0];
    expect(entry.positions).toEqual([0, 2]);
    expect(entry.frequency).toBe(2);
    expect(index.getDocumentCount()).toBe(1);
  });

  it('removes documents and cleans up posting lists', () => {
    const index = new MemoryInvertedIndex();
    const doc1 = makeDoc('doc-1', [
      { value: 'vector', type: 'word', position: 0, length: 6 },
      { value: 'index', type: 'word', position: 1, length: 5 },
    ]);
    const doc2 = makeDoc('doc-2', [{ value: 'vector', type: 'word', position: 0, length: 6 }]);
    index.addDocument(doc1);
    index.addDocument(doc2);

    index.removeDocument('doc-1');
    const list = index.getPostingList('index');
    expect(list).toBeUndefined();

    const vector = index.getPostingList('vector');
    expect(vector?.entries.map((e) => e.docId)).toEqual(['doc-2']);
    expect(index.getDocumentCount()).toBe(1);
  });

  it('updates documents without inflating doc count', () => {
    const index = new MemoryInvertedIndex();
    const doc = makeDoc('doc-1', [{ value: 'search', type: 'word', position: 0, length: 6 }]);
    index.addDocument(doc);
    index.updateDocument(
      makeDoc('doc-1', [
        { value: 'search', type: 'word', position: 0, length: 6 },
        { value: 'engine', type: 'word', position: 1, length: 6 },
      ]),
    );

    expect(index.getDocumentCount()).toBe(1);
    expect(index.getPostingList('engine')?.documentFrequency).toBe(1);
    expect(index.search(['search', 'engine']).get('doc-1')).toBe(2);
  });

  it('handles empty documents without creating postings', () => {
    const index = new MemoryInvertedIndex();
    index.addDocument(makeDoc('doc-empty', []));
    expect(index.getDocumentCount()).toBe(1);
    expect(index.getPostingList('anything')).toBeUndefined();
    expect(index.search(['missing']).size).toBe(0);
  });
});
