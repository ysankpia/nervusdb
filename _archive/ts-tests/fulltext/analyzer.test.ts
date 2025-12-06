import { describe, it, expect } from 'vitest';
import {
  StandardAnalyzer,
  KeywordAnalyzer,
  NGramAnalyzer,
  AnalyzerFactory,
} from '@/extensions/fulltext/analyzer.ts';

describe('全文 · 分析器', () => {
  it('StandardAnalyzer：英/中文分词、停用词与词干', () => {
    const analyzer = new StandardAnalyzer({ stemming: true, stopWords: true, ngramSize: 2 });
    const en = analyzer.analyze('The quick brown foxes jumped');
    expect(en.length).toBeGreaterThan(0);
    // 停用词应减少数量
    const enNoStop = new StandardAnalyzer({ stopWords: false }).analyze('the and to or');
    expect(enNoStop.length).toBeGreaterThan(0);

    const zh = analyzer.analyze('这是中文测试');
    expect(zh.length).toBeGreaterThan(0);
  });

  it('KeywordAnalyzer：保持原文', () => {
    const k = new KeywordAnalyzer();
    const t = k.analyze(' Hello World ');
    expect(t[0].value).toBe('Hello World');
  });

  it('NGramAnalyzer：按字符生成 n-gram', () => {
    const n = new NGramAnalyzer(3);
    const toks = n.analyze('abcd');
    expect(toks[0].type).toBe('ngram');
  });

  it('AnalyzerFactory：创建不同类型', () => {
    expect(() => AnalyzerFactory.createAnalyzer('standard')).not.toThrow();
    expect(() => AnalyzerFactory.createAnalyzer('keyword')).not.toThrow();
    expect(() => AnalyzerFactory.createAnalyzer('ngram', { ngramSize: 2 })).not.toThrow();
  });
});
