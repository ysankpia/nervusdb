import { describe, it, expect } from 'vitest';
import { MemoryInvertedIndex } from '@/extensions/fulltext/invertedIndex.ts';
import { MemoryDocumentCorpus } from '@/extensions/fulltext/corpus.ts';
import { TFIDFScorer, BM25Scorer, CompositeScorer } from '@/extensions/fulltext/scorer.ts';
import {
  QueryParser,
  FullTextQueryEngine,
  SearchHighlighter,
  FuzzySearchProcessor,
} from '@/extensions/fulltext/query.ts';
import { StandardAnalyzer } from '@/extensions/fulltext/analyzer.ts';
import { FullTextSearchFactory } from '@/extensions/fulltext/engine.ts';

describe('全文 · 索引/语料/评分/查询/引擎', () => {
  it('索引与语料：添加文档与基本搜索', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);
    const analyzer = new StandardAnalyzer({ stemming: false, stopWords: false });

    const mkDoc = (id: string, text: string) => ({
      id,
      fields: new Map([['content', text]]),
      tokens: analyzer.analyze(text),
      timestamp: new Date(),
    });
    const d1 = mkDoc('1', 'hello world');
    const d2 = mkDoc('2', 'hello synapse');
    idx.addDocument(d1);
    idx.addDocument(d2);
    corpus.addDocument(d1);
    corpus.addDocument(d2);

    const list = idx.getPostingList('hello');
    expect(list?.documentFrequency).toBe(2);

    const scorer = new TFIDFScorer();
    const tfidfScore = scorer.calculateScore(['hello'], d1, corpus);
    expect(Number.isFinite(tfidfScore)).toBe(true);
    const bm25 = new BM25Scorer();
    const bmScore = bm25.calculateScore(['hello'], d1, corpus);
    expect(Number.isFinite(bmScore)).toBe(true);

    const engine = new FullTextQueryEngine(
      idx,
      corpus,
      new CompositeScorer([{ scorer: bm25, weight: 1 }]),
      analyzer,
    );
    const res = engine.search('hello', { maxResults: 10, highlight: true });
    expect(Array.isArray(res)).toBe(true);

    const hl = new SearchHighlighter();
    const frags = hl.generateHighlights('1', ['hello'], corpus, { maxFragments: 1 });
    expect(Array.isArray(frags)).toBe(true);

    const fz = new FuzzySearchProcessor(idx, 1);
    expect(fz.fuzzySearch('helo') instanceof Set).toBe(true);
    expect(fz.wildcardSearch('he*') instanceof Set).toBe(true);

    const parser = new QueryParser(analyzer);
    expect(parser.parseQuery('"hello world"').length).toBe(1);
  });

  it('FullTextSearchEngine：创建索引/索引文档/搜索/建议/统计', async () => {
    const engine = FullTextSearchFactory.createEngine();
    await engine.createIndex(
      'idx',
      FullTextSearchFactory.createEnglishConfig(['title', 'content']),
    );
    await engine.indexDocument('idx', {
      id: 'doc1',
      fields: new Map([
        ['title', 'Hello'],
        ['content', 'Hello NervusDB'],
      ]),
      tokens: [],
      timestamp: new Date(),
    });
    const res = await engine.search('idx', 'Hello', { maxResults: 5 });
    expect(Array.isArray(res)).toBe(true);
    const sug = await engine.suggest('idx', 'He', 3);
    expect(Array.isArray(sug)).toBe(true);
    const stats = await engine.getIndexStats('idx');
    expect(stats.documentCount).toBeGreaterThan(0);
  });
});
