/**
 * 全文搜索引擎主入口
 *
 * 整合所有全文搜索组件，提供统一的搜索API
 */

import type {
  FullTextConfig,
  FullTextSearchEngine,
  Document,
  SearchOptions,
  SearchResult,
  IndexStats,
  Query,
} from './types.js';

import { AnalyzerFactory } from './analyzer.js';

import { MemoryInvertedIndex } from './invertedIndex.js';
import { MemoryDocumentCorpus } from './corpus.js';

import { ScorerFactory } from './scorer.js';

import { FullTextQueryEngine } from './query.js';

/**
 * 全文搜索索引管理器
 */
class FullTextIndex {
  public readonly name: string;
  public readonly config: FullTextConfig;
  public readonly invertedIndex: MemoryInvertedIndex;
  public readonly corpus: MemoryDocumentCorpus;
  public readonly analyzer: import('./types.js').TextAnalyzer;
  public readonly scorer: import('./types.js').RelevanceScorer;
  public readonly queryEngine: FullTextQueryEngine;

  private documentCount = 0;
  private lastUpdated: Date;

  constructor(name: string, config: FullTextConfig) {
    this.name = name;
    this.config = config;
    this.lastUpdated = new Date();

    // 初始化组件
    this.invertedIndex = new MemoryInvertedIndex();
    this.corpus = new MemoryDocumentCorpus(this.invertedIndex);
    this.analyzer = AnalyzerFactory.createAnalyzer(config.analyzer, {
      stemming: config.stemming,
      stopWords: config.stopWords,
      ngramSize: config.ngramSize,
    });
    this.scorer = ScorerFactory.createDefaultScorer();
    this.queryEngine = new FullTextQueryEngine(
      this.invertedIndex,
      this.corpus,
      this.scorer,
      this.analyzer,
    );
  }

  /**
   * 添加文档到索引
   */
  indexDocument(doc: Document): void {
    // 分析文档内容
    const analyzedDoc = this.analyzeDocument(doc);

    // 添加到索引
    this.invertedIndex.addDocument(analyzedDoc);
    this.corpus.addDocument(analyzedDoc);

    this.documentCount++;
    this.lastUpdated = new Date();
  }

  /**
   * 从索引中删除文档
   */
  removeDocument(docId: string): void {
    this.invertedIndex.removeDocument(docId);
    this.documentCount--;
    this.lastUpdated = new Date();
  }

  /**
   * 搜索
   */
  search(query: string | Query, options?: SearchOptions): Promise<SearchResult[]> {
    const queryString = typeof query === 'string' ? query : this.queryToString(query);
    return Promise.resolve(this.queryEngine.search(queryString, options));
  }

  /**
   * 获取搜索建议
   */
  async suggest(prefix: string, count: number): Promise<string[]> {
    return await this.queryEngine.suggest(prefix, count);
  }

  /**
   * 获取索引统计信息
   */
  getStats(): IndexStats {
    const indexStats = this.invertedIndex.getStats();

    return {
      name: this.name,
      documentCount: this.documentCount,
      uniqueTerms: indexStats.terms,
      indexSize: indexStats.indexSize,
      lastUpdated: this.lastUpdated,
    };
  }

  /**
   * 分析文档内容
   */
  private analyzeDocument(doc: Document): Document {
    const allTokens: import('./types.js').Token[] = [];

    // 分析每个字段
    for (const [fieldName, fieldContent] of doc.fields) {
      // 只分析配置中指定的字段
      if (this.config.fields.length === 0 || this.config.fields.includes(fieldName)) {
        const fieldTokens = this.analyzer.analyze(fieldContent, this.config.language);
        allTokens.push(...fieldTokens);
      }
    }

    return {
      ...doc,
      tokens: allTokens,
    };
  }

  /**
   * 将Query对象转换为字符串
   */
  private queryToString(query: Query): string {
    // 简化实现，实际应该有完整的序列化逻辑
    return typeof query.value === 'string' ? query.value : JSON.stringify(query.value);
  }
}

/**
 * 全文搜索引擎实现
 */
export class FullTextSearchEngineImpl implements FullTextSearchEngine {
  private indexes = new Map<string, FullTextIndex>();

  /**
   * 创建全文索引
   */
  createIndex(name: string, config: FullTextConfig): Promise<void> {
    if (this.indexes.has(name)) {
      throw new Error(`Index '${name}' already exists`);
    }

    const index = new FullTextIndex(name, config);
    this.indexes.set(name, index);
    return Promise.resolve();
  }

  /**
   * 删除全文索引
   */
  dropIndex(name: string): Promise<void> {
    if (!this.indexes.has(name)) {
      throw new Error(`Index '${name}' does not exist`);
    }

    this.indexes.delete(name);
    return Promise.resolve();
  }

  /**
   * 添加文档到索引
   */
  indexDocument(indexName: string, doc: Document): Promise<void> {
    const index = this.getIndex(indexName);
    index.indexDocument(doc);
    return Promise.resolve();
  }

  /**
   * 从索引中删除文档
   */
  removeDocument(indexName: string, docId: string): Promise<void> {
    const index = this.getIndex(indexName);
    index.removeDocument(docId);
    return Promise.resolve();
  }

  /**
   * 执行搜索
   */
  search(
    indexName: string,
    query: string | Query,
    options?: SearchOptions,
  ): Promise<SearchResult[]> {
    const index = this.getIndex(indexName);
    return Promise.resolve(index.search(query, options));
  }

  /**
   * 搜索建议
   */
  suggest(indexName: string, prefix: string, count: number): Promise<string[]> {
    const index = this.getIndex(indexName);
    return Promise.resolve(index.suggest(prefix, count));
  }

  /**
   * 获取索引统计信息
   */
  getIndexStats(indexName: string): Promise<IndexStats> {
    const index = this.getIndex(indexName);
    return Promise.resolve(index.getStats());
  }

  /**
   * 列出所有索引
   */
  listIndexes(): string[] {
    return Array.from(this.indexes.keys());
  }

  /**
   * 检查索引是否存在
   */
  hasIndex(name: string): boolean {
    return this.indexes.has(name);
  }

  /**
   * 获取索引实例
   */
  private getIndex(name: string): FullTextIndex {
    const index = this.indexes.get(name);
    if (!index) {
      throw new Error(`Index '${name}' does not exist`);
    }
    return index;
  }
}

/**
 * 全文搜索工厂类
 */
export class FullTextSearchFactory {
  /**
   * 创建搜索引擎实例
   */
  static createEngine(): FullTextSearchEngine {
    return new FullTextSearchEngineImpl();
  }

  /**
   * 创建默认配置
   */
  static createDefaultConfig(fields: string[]): FullTextConfig {
    return {
      fields,
      language: 'auto',
      analyzer: 'standard',
      stemming: true,
      stopWords: true,
      ngramSize: 2,
    };
  }

  /**
   * 创建中文配置
   */
  static createChineseConfig(fields: string[]): FullTextConfig {
    return {
      fields,
      language: 'zh',
      analyzer: 'standard',
      stemming: false, // 中文不需要词干提取
      stopWords: true,
      ngramSize: 2,
    };
  }

  /**
   * 创建英文配置
   */
  static createEnglishConfig(fields: string[]): FullTextConfig {
    return {
      fields,
      language: 'en',
      analyzer: 'standard',
      stemming: true,
      stopWords: true,
      ngramSize: 2,
    };
  }

  /**
   * 创建性能优化配置（适合大规模数据）
   */
  static createPerformanceConfig(fields: string[]): FullTextConfig {
    return {
      fields,
      language: 'auto',
      analyzer: 'keyword', // 使用更快的关键词分析器
      stemming: false, // 禁用词干提取以提高速度
      stopWords: false, // 禁用停用词过滤
      ngramSize: 1, // 减少N-gram大小
    };
  }
}

/**
 * 批处理工具
 */
export class FullTextBatchProcessor {
  private engine: FullTextSearchEngine;

  constructor(engine: FullTextSearchEngine) {
    this.engine = engine;
  }

  /**
   * 批量索引文档
   */
  async batchIndex(
    indexName: string,
    documents: Document[],
    batchSize: number = 1000,
  ): Promise<void> {
    for (let i = 0; i < documents.length; i += batchSize) {
      const batch = documents.slice(i, i + batchSize);

      // 批量处理
      const promises = batch.map((doc) => this.engine.indexDocument(indexName, doc));
      await Promise.all(promises);

      // 进度反馈
      console.log(
        `Indexed ${Math.min(i + batchSize, documents.length)} of ${documents.length} documents`,
      );
    }
  }

  /**
   * 批量搜索
   */
  async batchSearch(
    indexName: string,
    queries: string[],
    options?: SearchOptions,
  ): Promise<SearchResult[][]> {
    const promises = queries.map((query) => this.engine.search(indexName, query, options));

    return await Promise.all(promises);
  }
}

/**
 * 搜索性能监控器
 */
export class SearchPerformanceMonitor {
  private metrics = new Map<
    string,
    {
      totalQueries: number;
      totalTime: number;
      averageTime: number;
      slowQueries: Array<{ query: string; time: number; timestamp: Date }>;
    }
  >();

  /**
   * 监控搜索性能
   */
  async monitorSearch<T>(indexName: string, query: string, searchFn: () => Promise<T>): Promise<T> {
    const startTime = Date.now();

    try {
      const result = await searchFn();
      const duration = Date.now() - startTime;

      this.recordMetrics(indexName, query, duration);

      return result;
    } catch (error) {
      const duration = Date.now() - startTime;
      this.recordMetrics(indexName, query, duration, true);
      throw error;
    }
  }

  /**
   * 记录性能指标
   */
  private recordMetrics(
    indexName: string,
    query: string,
    duration: number,
    isError: boolean = false,
  ): void {
    void isError;
    if (!this.metrics.has(indexName)) {
      this.metrics.set(indexName, {
        totalQueries: 0,
        totalTime: 0,
        averageTime: 0,
        slowQueries: [],
      });
    }

    const metrics = this.metrics.get(indexName)!;
    metrics.totalQueries++;
    metrics.totalTime += duration;
    metrics.averageTime = metrics.totalTime / metrics.totalQueries;

    // 记录慢查询（>1000ms）
    if (duration > 1000) {
      metrics.slowQueries.push({
        query,
        time: duration,
        timestamp: new Date(),
      });

      // 只保留最近的100条慢查询
      if (metrics.slowQueries.length > 100) {
        metrics.slowQueries.shift();
      }
    }
  }

  /**
   * 获取性能报告
   */
  getPerformanceReport(indexName: string): unknown {
    return this.metrics.get(indexName) || null;
  }

  /**
   * 清理性能数据
   */
  clearMetrics(indexName?: string): void {
    if (indexName) {
      this.metrics.delete(indexName);
    } else {
      this.metrics.clear();
    }
  }
}

// 导出主要类和工厂
export { FullTextIndex, FullTextSearchEngineImpl as FullTextSearchEngine };
