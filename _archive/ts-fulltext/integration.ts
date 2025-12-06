/**
 * NervusDB 全文搜索集成
 *
 * 提供便捷的方法将全文搜索功能集成到 NervusDB 实例中
 */

import { NervusDB } from '../../synapseDb.js';
import {
  NervusDBFullTextExtension,
  NervusDBFullTextExtensionFactory,
  FullTextExtensionConfig,
  FullTextSearchResultWithFacts,
} from './synapsedbExtension.js';

import type { SearchOptions, SearchResult } from './types.js';

/**
 * 扩展 NervusDB 类的接口，添加全文搜索方法
 */
declare module '../../synapseDb.js' {
  interface NervusDB {
    /** 全文搜索扩展实例 */
    fullText?: NervusDBFullTextExtension;
    /** 内部：确保已启用全文搜索 */
    ensureFullTextEnabled(): void;

    /** 启用全文搜索功能 */
    enableFullTextSearch(config?: FullTextExtensionConfig): Promise<void>;

    /** 搜索三元组事实 */
    searchFacts(query: string, options?: SearchOptions): Promise<FullTextSearchResultWithFacts[]>;

    /** 搜索节点 */
    searchNodes(query: string, options?: SearchOptions): Promise<SearchResult[]>;

    /** 搜索边 */
    searchEdges(query: string, options?: SearchOptions): Promise<SearchResult[]>;

    /** 全局搜索 */
    globalSearch(
      query: string,
      options?: SearchOptions,
    ): Promise<{
      facts: FullTextSearchResultWithFacts[];
      nodes: SearchResult[];
      edges: SearchResult[];
    }>;

    /** 获取搜索建议 */
    getSearchSuggestions(
      prefix: string,
      count?: number,
    ): Promise<{
      facts: string[];
      nodes: string[];
      edges: string[];
    }>;

    /** 获取全文搜索统计信息 */
    getFullTextStats(): Promise<{ indexes: unknown; performance: unknown }>;

    /** 重建全文索引 */
    rebuildFullTextIndexes(): Promise<void>;
  }
}

/**
 * 为 NervusDB 添加全文搜索方法
 */
NervusDB.prototype.enableFullTextSearch = async function (
  this: NervusDB,
  config?: FullTextExtensionConfig,
): Promise<void> {
  if (this.fullText) {
    console.warn('Full-text search is already enabled for this NervusDB instance');
    return;
  }

  try {
    this.fullText = await NervusDBFullTextExtensionFactory.create(this, config);
    console.log('Full-text search enabled successfully');
  } catch (error) {
    console.error('Failed to enable full-text search:', error);
    throw error;
  }
};

NervusDB.prototype.searchFacts = async function (
  this: NervusDB,
  query: string,
  options?: SearchOptions,
): Promise<FullTextSearchResultWithFacts[]> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  return await this.fullText!.searchFacts(query, options);
};

NervusDB.prototype.searchNodes = async function (
  this: NervusDB,
  query: string,
  options?: SearchOptions,
): Promise<SearchResult[]> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  return await this.fullText!.searchNodes(query, options);
};

NervusDB.prototype.searchEdges = async function (
  this: NervusDB,
  query: string,
  options?: SearchOptions,
): Promise<SearchResult[]> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  return await this.fullText!.searchEdges(query, options);
};

NervusDB.prototype.globalSearch = async function (
  this: NervusDB,
  query: string,
  options?: SearchOptions,
): Promise<{
  facts: FullTextSearchResultWithFacts[];
  nodes: SearchResult[];
  edges: SearchResult[];
}> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  return await this.fullText!.globalSearch(query, options);
};

NervusDB.prototype.getSearchSuggestions = async function (
  this: NervusDB,
  prefix: string,
  count: number = 10,
): Promise<{
  facts: string[];
  nodes: string[];
  edges: string[];
}> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  return await this.fullText!.getSuggestions(prefix, count);
};

NervusDB.prototype.getFullTextStats = async function (
  this: NervusDB,
): Promise<{ indexes: unknown; performance: unknown }> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  const [indexStats, performanceReport] = await Promise.all([
    this.fullText!.getIndexStats(),
    Promise.resolve(this.fullText!.getPerformanceReport()),
  ]);

  return {
    indexes: indexStats,
    performance: performanceReport,
  };
};

NervusDB.prototype.rebuildFullTextIndexes = async function (this: NervusDB): Promise<void> {
  (this as NervusDB & { ensureFullTextEnabled: () => void }).ensureFullTextEnabled();
  await this.fullText!.rebuildIndexes();
};

/**
 * 添加私有辅助方法检查全文搜索是否启用
 */
(
  NervusDB.prototype as unknown as {
    ensureFullTextEnabled: (this: NervusDB) => void;
  }
).ensureFullTextEnabled = function (this: NervusDB): void {
  if (!this.fullText) {
    throw new Error('Full-text search is not enabled. Call enableFullTextSearch() first.');
  }
};

/**
 * 全文搜索工具函数
 */
export class FullTextSearchUtils {
  /**
   * 为多个 NervusDB 实例批量启用全文搜索
   */
  static async enableForMultiple(
    databases: NervusDB[],
    config?: FullTextExtensionConfig,
  ): Promise<void> {
    const promises = databases.map((db) => db.enableFullTextSearch(config));
    await Promise.all(promises);
  }

  /**
   * 跨多个数据库搜索
   */
  static async searchAcrossDatabases(
    databases: NervusDB[],
    query: string,
    options?: SearchOptions,
  ): Promise<
    Array<{
      database: NervusDB;
      results: {
        facts: FullTextSearchResultWithFacts[];
        nodes: SearchResult[];
        edges: SearchResult[];
      };
    }>
  > {
    const promises = databases.map(async (db) => ({
      database: db,
      results: await db.globalSearch(query, options),
    }));

    return await Promise.all(promises);
  }

  /**
   * 创建搜索摘要
   */
  static createSearchSummary(results: {
    facts: FullTextSearchResultWithFacts[];
    nodes: SearchResult[];
    edges: SearchResult[];
  }): {
    totalResults: number;
    factCount: number;
    nodeCount: number;
    edgeCount: number;
    topScores: number[];
    avgScore: number;
  } {
    const allScores = [
      ...results.facts.map((r) => r.searchResult.score),
      ...results.nodes.map((r) => r.score),
      ...results.edges.map((r) => r.score),
    ];

    const topScores = allScores.sort((a, b) => b - a).slice(0, 5);

    const avgScore =
      allScores.length > 0
        ? allScores.reduce((sum, score) => sum + score, 0) / allScores.length
        : 0;

    return {
      totalResults: results.facts.length + results.nodes.length + results.edges.length,
      factCount: results.facts.length,
      nodeCount: results.nodes.length,
      edgeCount: results.edges.length,
      topScores,
      avgScore,
    };
  }

  /**
   * 高亮搜索结果中的关键词
   */
  static highlightKeywords(
    text: string,
    keywords: string[],
    options: {
      preTag?: string;
      postTag?: string;
      caseSensitive?: boolean;
    } = {},
  ): string {
    const { preTag = '<mark>', postTag = '</mark>', caseSensitive = false } = options;

    let highlightedText = text;

    for (const keyword of keywords) {
      const flags = caseSensitive ? 'g' : 'gi';
      const regex = new RegExp(`(${this.escapeRegex(keyword)})`, flags);
      highlightedText = highlightedText.replace(regex, `${preTag}$1${postTag}`);
    }

    return highlightedText;
  }

  /**
   * 转义正则表达式特殊字符
   */
  private static escapeRegex(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }

  /**
   * 解析查询字符串为关键词
   */
  static parseQueryKeywords(query: string): string[] {
    return query
      .toLowerCase()
      .replace(/[^\w\s\u4e00-\u9fa5]/g, ' ') // 保留中文字符
      .split(/\s+/)
      .filter((word) => word.length > 0);
  }

  /**
   * 创建搜索选项的便捷方法
   */
  static createSearchOptions(overrides: Partial<SearchOptions> = {}): SearchOptions {
    return {
      fields: [],
      fuzzy: false,
      maxEditDistance: 2,
      minScore: 0,
      maxResults: 100,
      sortBy: 'relevance',
      highlight: true,
      highlightFragments: 3,
      ...overrides,
    };
  }

  /**
   * 创建高精度搜索选项
   */
  static createHighPrecisionOptions(): SearchOptions {
    return this.createSearchOptions({
      fuzzy: false,
      minScore: 0.5,
      maxResults: 50,
      sortBy: 'relevance',
    });
  }

  /**
   * 创建模糊搜索选项
   */
  static createFuzzySearchOptions(): SearchOptions {
    return this.createSearchOptions({
      fuzzy: true,
      maxEditDistance: 2,
      minScore: 0.1,
      maxResults: 200,
      sortBy: 'relevance',
    });
  }
}

/**
 * 导出集成相关的功能
 */
export { NervusDBFullTextExtension, NervusDBFullTextExtensionFactory };
