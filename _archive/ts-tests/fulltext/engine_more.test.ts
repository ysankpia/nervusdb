import { describe, it, expect } from 'vitest';
import {
  FullTextSearchFactory,
  FullTextSearchEngine,
  FullTextBatchProcessor,
  SearchPerformanceMonitor,
} from '@/extensions/fulltext/engine.ts';

describe('全文引擎 · 工厂/批处理/监控', () => {
  it('工厂配置：默认/中文/英文/性能', () => {
    const d = FullTextSearchFactory.createDefaultConfig(['title']);
    expect(d.stemming).toBe(true);
    const zh = FullTextSearchFactory.createChineseConfig(['content']);
    expect(zh.language).toBe('zh');
    const en = FullTextSearchFactory.createEnglishConfig(['content']);
    expect(en.language).toBe('en');
    const perf = FullTextSearchFactory.createPerformanceConfig(['content']);
    expect(perf.analyzer).toBe('keyword');
  });

  it('引擎：索引生命周期/查询与建议/统计', async () => {
    const engine: FullTextSearchEngine = FullTextSearchFactory.createEngine();
    await engine.createIndex(
      'idx',
      FullTextSearchFactory.createDefaultConfig(['title', 'content']),
    );
    expect(engine.hasIndex('idx')).toBe(true);
    expect(engine.listIndexes()).toContain('idx');
    await engine.indexDocument('idx', {
      id: '1',
      fields: new Map([
        ['title', 'Alpha'],
        ['content', 'Alpha Beta'],
      ]),
      tokens: [],
      timestamp: new Date(),
    });
    const r = await engine.search('idx', 'Alpha', { maxResults: 5 });
    expect(Array.isArray(r)).toBe(true);
    const s = await engine.suggest('idx', 'Al', 5);
    expect(Array.isArray(s)).toBe(true);
    const st = await engine.getIndexStats('idx');
    expect(st.documentCount).toBeGreaterThan(0);

    await engine.dropIndex('idx');
    expect(engine.hasIndex('idx')).toBe(false);
  });

  it('批处理：batchIndex/batchSearch', async () => {
    const engine = FullTextSearchFactory.createEngine();
    await engine.createIndex('b', FullTextSearchFactory.createDefaultConfig(['content']));
    const bp = new FullTextBatchProcessor(engine);
    const docs = Array.from({ length: 3 }).map((_, i) => ({
      id: String(i + 1),
      fields: new Map([['content', `doc${i + 1}`]]),
      tokens: [],
      timestamp: new Date(),
    }));
    await bp.batchIndex('b', docs, 1);
    const results = await bp.batchSearch('b', ['doc1', 'doc2']);
    expect(results.length).toBe(2);
  });

  it('性能监控：monitorSearch/getPerformanceReport/clear', async () => {
    const mon = new SearchPerformanceMonitor();
    const out = await mon.monitorSearch('idx', 'q', async () => 'ok');
    expect(out).toBe('ok');
    const report = mon.getPerformanceReport('idx') as any;
    expect(report.totalQueries).toBeGreaterThan(0);
    mon.clearMetrics('idx');
    expect(mon.getPerformanceReport('idx')).toBeNull();
  });
});
