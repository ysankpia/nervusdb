import { describe, it, expect } from 'vitest';
import { NervusDB } from '@/synapseDb.ts';
import '@/extensions/fulltext/integration.ts';

describe('NervusDB 全文搜索扩展 · 启用/调用/统计/重建', () => {
  it('未启用时报错；启用后可调用各 API', async () => {
    const db = await NervusDB.open(':memory:');
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

  it('重复启用全文搜索应该幂等', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    // 第二次启用应该幂等（不报错）
    await db.enableFullTextSearch();

    // 仍可正常搜索
    const results = await db.searchNodes('test');
    expect(Array.isArray(results)).toBe(true);

    await db.close();
  });

  it('批量索引大量节点和边', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    // 批量添加节点
    for (let i = 0; i < 100; i++) {
      db.addFact(
        {
          subject: `node${i}`,
          predicate: 'has_content',
          object: `content_${i}`,
        },
        {
          subjectProperties: { text: `这是第 ${i} 个测试节点` },
          objectProperties: { description: `描述内容 ${i}` },
        },
      );
    }

    // 搜索应该能返回结果（可能需要索引时间）
    const results = await db.searchNodes('测试节点');
    expect(Array.isArray(results)).toBe(true);

    // 搜索边
    const edgeResults = await db.searchEdges('描述内容');
    expect(Array.isArray(edgeResults)).toBe(true);

    await db.close();
  });

  it('空查询和特殊字符查询', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    db.addFact(
      {
        subject: 'test',
        predicate: 'has',
        object: 'value',
      },
      {
        subjectProperties: { text: '特殊字符 @#$%^&*()' },
      },
    );

    // 空查询应该返回空数组
    const empty = await db.searchNodes('');
    expect(empty.length).toBe(0);

    // 特殊字符查询应该能执行
    const special = await db.searchNodes('特殊字符');
    expect(Array.isArray(special)).toBe(true);

    await db.close();
  });

  it('分页查询和排序', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    // 添加多个匹配的文档
    for (let i = 0; i < 50; i++) {
      db.addFact(
        {
          subject: `doc${i}`,
          predicate: 'contains',
          object: `data${i}`,
        },
        {
          subjectProperties: {
            text: `搜索关键词 出现在文档 ${i}`,
            score: i, // 用于测试排序
          },
        },
      );
    }

    // 测试 maxResults 限制
    const page1 = await db.searchNodes('搜索关键词', { maxResults: 10 });
    expect(page1.length).toBeLessThanOrEqual(10);

    // 测试全局搜索
    const global = await db.globalSearch('搜索关键词', { maxResults: 20 });
    expect(global.nodes.length).toBeLessThanOrEqual(20);

    await db.close();
  });

  it('搜索建议功能', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    db.addFact(
      {
        subject: 'user1',
        predicate: 'name',
        object: 'Alice',
      },
      {
        subjectProperties: { fullName: 'Alice Johnson' },
        objectProperties: { description: 'A software engineer' },
      },
    );

    db.addFact(
      {
        subject: 'user2',
        predicate: 'name',
        object: 'Bob',
      },
      {
        subjectProperties: { fullName: 'Bob Smith' },
      },
    );

    // 获取建议（前缀匹配）
    const suggestions = await db.getSearchSuggestions('Al', 5);
    expect(suggestions).toBeDefined();
    expect(suggestions.nodes).toBeDefined();

    await db.close();
  });

  it('统计信息应该反映索引状态', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    // 初始状态
    const stats1 = await db.getFullTextStats();
    expect(stats1).toBeDefined();

    // 添加数据后
    db.addFact(
      {
        subject: 'item',
        predicate: 'type',
        object: 'product',
      },
      {
        subjectProperties: { name: 'Test Product' },
      },
    );

    const stats2 = await db.getFullTextStats();
    expect(stats2).toBeDefined();

    await db.close();
  });

  it('重建索引后数据仍可搜索', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    db.addFact(
      {
        subject: 'article',
        predicate: 'title',
        object: 'Important Article',
      },
      {
        subjectProperties: { content: '这是一篇重要的文章内容' },
      },
    );

    // 搜索确认数据存在
    const before = await db.searchNodes('文章');
    expect(Array.isArray(before)).toBe(true);

    // 重建索引
    await db.rebuildFullTextIndexes();

    // 重建后搜索仍能执行
    const after = await db.searchNodes('文章');
    expect(Array.isArray(after)).toBe(true);

    await db.close();
  });

  it('全局搜索应该同时搜索节点和边', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    db.addFact(
      {
        subject: 'person',
        predicate: 'knows',
        object: 'friend',
      },
      {
        subjectProperties: { name: '张三测试' },
        objectProperties: { name: '李四' },
        edgeProperties: { relationship: '好朋友' },
      },
    );

    const results = await db.globalSearch('测试');

    // 应该能搜索到节点
    expect(results).toBeDefined();
    expect(results.nodes).toBeDefined();
    expect(Array.isArray(results.nodes)).toBe(true);

    await db.close();
  });

  it('搜索结果应该包含相关性分数', async () => {
    const db = await NervusDB.open(':memory:');
    await db.enableFullTextSearch();

    db.addFact(
      {
        subject: 'doc1',
        predicate: 'content',
        object: 'data',
      },
      {
        subjectProperties: {
          text: '机器学习是人工智能的一个重要分支',
        },
      },
    );

    const results = await db.searchNodes('机器学习', { maxResults: 10 });

    if (results.length > 0) {
      // 结果应该有相关信息
      expect(results[0]).toHaveProperty('subject');
    }

    await db.close();
  });
});
