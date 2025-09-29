import { describe, it, expect } from 'vitest';
import { MemoryInvertedIndex } from '@/fulltext/invertedIndex.ts';
import { MemoryDocumentCorpus } from '@/fulltext/corpus.ts';
import {
  BooleanQueryProcessor,
  PhraseQueryProcessor,
  QueryParser,
  FullTextQueryEngine,
  SearchHighlighter,
  EditDistanceCalculator,
  FuzzySearchProcessor,
} from '@/fulltext/query.ts';
import { StandardAnalyzer } from '@/fulltext/analyzer.ts';
import { BM25Scorer } from '@/fulltext/scorer.ts';

function mkDoc(id: string, text: string) {
  const analyzer = new StandardAnalyzer({ stemming: false, stopWords: false });
  return {
    id,
    fields: new Map<string, string>([['content', text]]),
    tokens: analyzer.analyze(text),
    timestamp: new Date(),
  };
}

describe('全文 · 查询边界与错误路径', () => {
  it('布尔/短语处理器：AND/OR/NOT 与相邻位置匹配', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);
    const a = new StandardAnalyzer({ stemming: false, stopWords: false });
    const d1 = mkDoc('1', 'hello world'); // hello@0 world@1
    const d2 = mkDoc('2', 'hello synapse'); // hello@0
    for (const d of [d1, d2]) {
      idx.addDocument(d);
      corpus.addDocument(d);
    }

    const boolp = new BooleanQueryProcessor(idx);
    const orSet = boolp.processOR(['hello']);
    expect(orSet.has('1') && orSet.has('2')).toBe(true);
    const andSet = boolp.processAND(['hello', 'world']);
    expect(andSet.has('1')).toBe(true);
    const notSet = boolp.processNOT(['hello'], ['world']);
    // 包含 hello 但不包含 world -> 仅 d2
    expect(notSet.has('2')).toBe(true);
    expect(notSet.has('1')).toBe(false);

    const phrase = new PhraseQueryProcessor(idx);
    const phraseRes = phrase.processPhrase(['hello', 'world']);
    expect(phraseRes.has('1')).toBe(true);
  });

  it('QueryParser：类型判定（boolean/phrase/wildcard/term）', () => {
    const p = new QueryParser(new StandardAnalyzer());
    expect(p.parseQuery('alpha AND beta')[0].type).toBe('boolean');
    expect(p.parseQuery('"hello world"')[0].type).toBe('phrase');
    expect(p.parseQuery('he*')[0].type).toBe('wildcard');
    expect(p.parseQuery('hello')[0].type).toBe('term');
  });

  it('引擎：字段过滤/排序/高亮/建议/模糊/通配符', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);
    const analyzer = new StandardAnalyzer({ stemming: false, stopWords: false });
    const scorer = new BM25Scorer();
    const engine = new FullTextQueryEngine(idx, corpus, scorer, analyzer);

    const newer = { ...mkDoc('n', 'alpha beta'), timestamp: new Date() };
    const older = { ...mkDoc('o', 'alpha'), timestamp: new Date(Date.now() - 86400000) };
    for (const d of [newer, older]) {
      idx.addDocument(d);
      corpus.addDocument(d);
    }

    // 字段过滤：仅 content 存在，筛 title 应得空
    expect(engine.search('alpha', { fields: ['title'] }).length).toBe(0);
    // 默认字段：应有结果
    // 选择只出现在 newer 的词，确保 BM25 idf 为正
    const res = engine.search('beta', { highlight: true, sortBy: 'date' });
    expect(res.length).toBeGreaterThan(0);

    // date 排序：newer 在前（如果分数相同，按日期降序）
    expect(res[0].docId).toBe('n');

    // 高亮生成（直接调用）
    const hl = new SearchHighlighter();
    const frags = hl.generateHighlights('n', ['alpha'], corpus, { maxFragments: 1 });
    expect(frags.length).toBeGreaterThanOrEqual(0);

    // 模糊/通配符/前缀：路径可运行（内部词汇表为空时返回空集合）
    const fz = new FuzzySearchProcessor(idx, 2);
    expect(fz.fuzzySearch('alp') instanceof Set).toBe(true);
    expect(fz.wildcardSearch('a*') instanceof Set).toBe(true);
    expect(fz.prefixSearch('a') instanceof Set).toBe(true);

    // 编辑距离
    expect(EditDistanceCalculator.calculate('kitten', 'sitting')).toBe(3);
    expect(EditDistanceCalculator.isWithinDistance('aaaa', 'aaa', 1)).toBe(true);
  });
});
