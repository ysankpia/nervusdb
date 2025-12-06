import { describe, it, expect } from 'vitest';
import { MemoryInvertedIndex } from '@/extensions/fulltext/invertedIndex.ts';
import { MemoryDocumentCorpus } from '@/extensions/fulltext/corpus.ts';
import { VectorSpaceScorer } from '@/extensions/fulltext/scorer.ts';

function mkDoc(id: string, tokens: string[]) {
  return {
    id,
    fields: new Map([['content', tokens.join(' ')]]),
    tokens: tokens.map((v, i) => ({
      value: v,
      type: 'word' as const,
      position: i,
      length: v.length,
    })),
  };
}

describe('VectorSpaceScorer · 余弦相似度', () => {
  it('向量模型：含查询全部词的文档得分更高', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);
    const d1 = mkDoc('1', ['a', 'b', 'a']);
    const d2 = mkDoc('2', ['a', 'a', 'a']);
    for (const d of [d1, d2]) {
      idx.addDocument(d);
      corpus.addDocument(d);
    }

    const scorer = new VectorSpaceScorer();
    const q = ['a', 'b'];
    const s1 = scorer.calculateScore(q, d1, corpus);
    const s2 = scorer.calculateScore(q, d2, corpus);
    expect(s1).toBeGreaterThan(0);
    expect(s1).toBeGreaterThan(s2); // 同时包含 a、b 的文档更接近查询
  });
});
