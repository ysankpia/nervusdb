import { describe, it, expect } from 'vitest';
import { MemoryInvertedIndex } from '@/fulltext/invertedIndex.ts';
import { MemoryDocumentCorpus } from '@/fulltext/corpus.ts';
import {
  TFIDFScorer,
  BM25Scorer,
  FieldWeightedScorer,
  TimeDecayScorer,
  CompositeScorer,
  ScorerFactory,
} from '@/fulltext/scorer.ts';

function mkDoc(id: string, tokens: string[], fields?: Record<string, string>) {
  return {
    id,
    fields: new Map<string, string>(Object.entries(fields ?? { content: tokens.join(' ') })),
    tokens: tokens.map((v, i) => ({
      value: v,
      type: 'word' as const,
      position: i,
      length: v.length,
    })),
    timestamp: new Date(),
  };
}

describe('全文 · 评分器数学验证', () => {
  it('TF-IDF 与 BM25 对拍（小语料，手算验证）', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);

    // 语料：
    // d1: hello world hello  (len=3)
    // d2: world              (len=1)
    // d3: synapse            (len=1)
    const d1 = mkDoc('d1', ['hello', 'world', 'hello']);
    const d2 = mkDoc('d2', ['world']);
    const d3 = mkDoc('d3', ['synapse']);
    for (const d of [d1, d2, d3]) {
      idx.addDocument(d);
      corpus.addDocument(d);
    }

    // N=3, 对 term=hello: n=1（仅 d1 包含）
    // TF‑IDF：tf = 1+ln(2)=1.6931；idf = ln(3/(1+1))=ln(1.5)=0.4055；
    // raw = 0.686；长度归一化：/sqrt(3)=/1.732；期望 ~0.396
    const tfidf = new TFIDFScorer();
    const tfidfScore = tfidf.calculateScore(['hello'], d1, corpus);
    expect(tfidfScore).toBeGreaterThan(0);
    expect(tfidfScore).toBeCloseTo(0.396, 2);

    // BM25：k1=1.2,b=0.75；idf = ln((3-1+0.5)/(1+0.5))=ln(1.6667)=0.5108；
    // tf=2；avgLen=(3+1+1)/3=1.6667；
    // num=2*(1.2+1)=4.4；den=2 + 1.2*(1-0.75 + 0.75*(3/1.6667)) ≈ 3.92；
    // score ~= 0.5108*(4.4/3.92)=0.572
    const bm25 = new BM25Scorer();
    const bmScore = bm25.calculateScore(['hello'], d1, corpus);
    expect(bmScore).toBeCloseTo(0.572, 2);
  });

  it('字段权重与时间衰减应影响分数（方向正确）', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);

    const doc = mkDoc('d', ['hello', 'world', 'hello'], {
      title: 'hello',
      content: 'hello world',
    });
    idx.addDocument(doc);
    corpus.addDocument(doc);
    // 增加不包含 hello 的文档以获得正的 idf
    const d2 = mkDoc('d2', ['world']);
    const d3 = mkDoc('d3', ['synapse']);
    idx.addDocument(d2);
    corpus.addDocument(d2);
    idx.addDocument(d3);
    corpus.addDocument(d3);

    const base = new BM25Scorer();
    const baseScore = base.calculateScore(['hello'], doc, corpus);

    const weighted = new FieldWeightedScorer(base);
    const weightedScore = weighted.calculateScore(['hello'], doc, corpus);
    expect(weightedScore).toBeGreaterThan(baseScore); // 标题权重提升

    // 时间衰减：较新的文档得分更高
    const newer = { ...doc, timestamp: new Date() };
    const older = { ...doc, timestamp: new Date(Date.now() - 30 * 24 * 3600 * 1000) }; // 30 天前
    const decay = new TimeDecayScorer(base, 0.1, 24 * 3600 * 1000);
    const sNew = decay.calculateScore(['hello'], newer, corpus);
    const sOld = decay.calculateScore(['hello'], older, corpus);
    expect(sNew).toBeGreaterThan(sOld);
  });

  it('组合评分器 = 权重加权和；工厂创建的类型可用', () => {
    const idx = new MemoryInvertedIndex();
    const corpus = new MemoryDocumentCorpus(idx);
    const d = mkDoc('d', ['a', 'a', 'b']);
    idx.addDocument(d);
    corpus.addDocument(d);

    const tfidf = new TFIDFScorer();
    const bm25 = new BM25Scorer();
    const tfScore = tfidf.calculateScore(['a'], d, corpus);
    const bmScore = bm25.calculateScore(['a'], d, corpus);
    const comp = new CompositeScorer([
      { scorer: tfidf, weight: 0.5 },
      { scorer: bm25, weight: 0.5 },
    ]);
    const compScore = comp.calculateScore(['a'], d, corpus);
    expect(compScore).toBeCloseTo(0.5 * tfScore + 0.5 * bmScore, 5);

    // 工厂方法
    expect(() => ScorerFactory.createScorer('tfidf')).not.toThrow();
    expect(() => ScorerFactory.createScorer('bm25')).not.toThrow();
    expect(() => ScorerFactory.createScorer('vector')).not.toThrow();
    expect(() =>
      ScorerFactory.createScorer('composite', { scorers: [{ scorer: tfidf, weight: 1 }] }),
    ).not.toThrow();
    expect(ScorerFactory.createDefaultScorer()).toBeTruthy();
  });
});
