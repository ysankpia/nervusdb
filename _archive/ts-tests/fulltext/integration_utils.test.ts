import { describe, it, expect } from 'vitest';
import { FullTextSearchUtils } from '@/extensions/fulltext/integration.ts';

describe('FullTextSearchUtils · 纯工具函数', () => {
  it('highlightKeywords/parseQueryKeywords', () => {
    const text = '这是技术文档。技术很好，文档也很好。';
    const kw = FullTextSearchUtils.parseQueryKeywords('技术 文档!');
    expect(kw).toEqual(['技术', '文档']);

    const hl = FullTextSearchUtils.highlightKeywords(text, kw, {
      preTag: '<b>',
      postTag: '</b>',
    });
    expect(hl.includes('<b>技术</b>')).toBe(true);
    expect(hl.includes('<b>文档</b>')).toBe(true);
  });

  it('create*SearchOptions 合并与预设', () => {
    const base = FullTextSearchUtils.createSearchOptions({ maxResults: 10, fuzzy: true });
    expect(base.maxResults).toBe(10);
    expect(base.fuzzy).toBe(true);

    const hi = FullTextSearchUtils.createHighPrecisionOptions();
    expect(hi.fuzzy).toBe(false);
    expect(hi.minScore).toBeGreaterThan(0);

    const fuzzy = FullTextSearchUtils.createFuzzySearchOptions();
    expect(fuzzy.fuzzy).toBe(true);
    expect(fuzzy.maxEditDistance).toBeGreaterThan(0);
  });
});
