import { describe, it, expect } from 'vitest';
import { SynapseDB } from '@/synapseDb.ts';
import '@/fulltext/integration.ts';

describe('SynapseDB 全文搜索扩展 · 启用/调用/统计/重建', () => {
  it('未启用时报错；启用后可调用各 API', async () => {
    const db = await SynapseDB.open(':memory:');
    // 未启用前调用应报错
    await expect(db.searchNodes('hello')).rejects.toThrow(/Full-text search is not enabled/);

    // 启用（使用默认配置）
    await db.enableFullTextSearch();

    // 空数据情况下调用搜索/建议/统计/重建不应抛错
    const nodes = await db.searchNodes('hello', { maxResults: 5 });
    expect(Array.isArray(nodes)).toBe(true);
    const edges = await db.searchEdges('hello');
    expect(Array.isArray(edges)).toBe(true);
    const facts = await db.searchFacts('hello');
    expect(Array.isArray(facts)).toBe(true);
    const all = await db.globalSearch('hello');
    expect(Array.isArray(all.nodes)).toBe(true);
    const sug = await db.getSearchSuggestions('he', 3);
    expect(Array.isArray(sug.nodes)).toBe(true);

    const stats = await db.getFullTextStats();
    expect(stats).toBeTruthy();
    await db.rebuildFullTextIndexes();

    await db.close();
  });
});
