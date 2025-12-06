/**
 * NervusDB 全文搜索扩展
 *
 * 为 NervusDB 添加全文搜索功能，支持三元组内容的高级文本检索
 */

import { NervusDB } from '../../synapseDb.js';
import { FactRecord } from '../../core/storage/persistentStore.js';

import {
  FullTextSearchFactory,
  FullTextBatchProcessor,
  SearchPerformanceMonitor,
} from './engine.js';
import type {
  FullTextSearchEngine,
  Document,
  SearchOptions,
  SearchResult,
  IndexStats,
} from './types.js';

/**
 * 全文搜索扩展配置
 */
export interface FullTextExtensionConfig {
  /** 默认索引名称 */
  defaultIndexName?: string;
  /** 启用性能监控 */
  enablePerformanceMonitoring?: boolean;
  /** 自动索引三元组内容 */
  autoIndexTriples?: boolean;
  /** 批处理大小 */
  batchSize?: number;
}

/**
 * 全文搜索结果与三元组结果的映射
 */
export interface FullTextSearchResultWithFacts {
  /** 搜索结果 */
  searchResult: SearchResult;
  /** 关联的三元组事实 */
  facts: FactRecord[];
  /** 匹配的字段内容 */
  matchedContent: string[];
}

/**
 * NervusDB 全文搜索扩展类
 */
export class NervusDBFullTextExtension {
  private searchEngine: FullTextSearchEngine;
  private batchProcessor: FullTextBatchProcessor;
  private performanceMonitor?: SearchPerformanceMonitor;
  private config: Required<FullTextExtensionConfig>;

  private readonly FACT_INDEX_NAME = 'facts';
  private readonly NODE_INDEX_NAME = 'nodes';
  private readonly EDGE_INDEX_NAME = 'edges';

  constructor(
    private db: NervusDB,
    config: FullTextExtensionConfig = {},
  ) {
    this.config = {
      defaultIndexName: 'default',
      enablePerformanceMonitoring: false,
      autoIndexTriples: true,
      batchSize: 1000,
      ...config,
    };

    this.searchEngine = FullTextSearchFactory.createEngine();
    this.batchProcessor = new FullTextBatchProcessor(this.searchEngine);

    if (this.config.enablePerformanceMonitoring) {
      this.performanceMonitor = new SearchPerformanceMonitor();
    }

    void this.initializeIndexes();
  }

  /**
   * 初始化索引
   */
  private async initializeIndexes(): Promise<void> {
    // 三元组事实索引
    await this.searchEngine.createIndex(
      this.FACT_INDEX_NAME,
      FullTextSearchFactory.createDefaultConfig(['subject', 'predicate', 'object']),
    );

    // 节点属性索引
    await this.searchEngine.createIndex(
      this.NODE_INDEX_NAME,
      FullTextSearchFactory.createDefaultConfig(['nodeValue', 'properties']),
    );

    // 边属性索引
    await this.searchEngine.createIndex(
      this.EDGE_INDEX_NAME,
      FullTextSearchFactory.createDefaultConfig(['properties']),
    );

    // 如果启用自动索引，索引现有数据
    if (this.config.autoIndexTriples) {
      await this.indexExistingData();
    }
  }

  /**
   * 索引现有的三元组数据
   */
  private async indexExistingData(): Promise<void> {
    try {
      const facts = this.db.listFacts();
      const documents: Document[] = [];

      for (const fact of facts) {
        // 索引三元组事实
        documents.push(this.factToDocument(fact));

        // 索引节点属性
        if (fact.subjectProperties && Object.keys(fact.subjectProperties).length > 0) {
          documents.push(this.nodeToDocument(fact.subject, fact.subjectProperties));
        }

        if (fact.objectProperties && Object.keys(fact.objectProperties).length > 0) {
          documents.push(this.nodeToDocument(fact.object, fact.objectProperties));
        }

        // 索引边属性
        if (fact.edgeProperties && Object.keys(fact.edgeProperties).length > 0) {
          documents.push(this.edgeToDocument(fact, fact.edgeProperties));
        }
      }

      // 批量索引
      if (documents.length > 0) {
        await this.batchProcessor.batchIndex(
          this.FACT_INDEX_NAME,
          documents,
          this.config.batchSize,
        );
      }

      console.log(`Successfully indexed ${documents.length} documents`);
    } catch (error) {
      console.error('Failed to index existing data:', error);
    }
  }

  /**
   * 将三元组事实转换为文档
   */
  private factToDocument(fact: FactRecord): Document {
    return {
      id: `fact_${fact.subjectId}_${fact.predicateId}_${fact.objectId}`,
      fields: new Map([
        ['subject', fact.subject],
        ['predicate', fact.predicate],
        ['object', fact.object],
        ['factType', 'triple'],
      ]),
      tokens: [], // 将由分析器生成
      timestamp: new Date(),
    };
  }

  /**
   * 将节点转换为文档
   */
  private nodeToDocument(nodeValue: string, properties: Record<string, unknown>): Document {
    const propertiesText = JSON.stringify(properties);

    return {
      id: `node_${nodeValue}`,
      fields: new Map([
        ['nodeValue', nodeValue],
        ['properties', propertiesText],
        ['nodeType', 'node'],
      ]),
      tokens: [],
      timestamp: new Date(),
    };
  }

  /**
   * 将边转换为文档
   */
  private edgeToDocument(fact: FactRecord, properties: Record<string, unknown>): Document {
    const propertiesText = JSON.stringify(properties);

    return {
      id: `edge_${fact.subjectId}_${fact.predicateId}_${fact.objectId}`,
      fields: new Map([
        ['subject', fact.subject],
        ['predicate', fact.predicate],
        ['object', fact.object],
        ['properties', propertiesText],
        ['edgeType', 'edge'],
      ]),
      tokens: [],
      timestamp: new Date(),
    };
  }

  /**
   * 搜索三元组事实
   */
  async searchFacts(
    query: string,
    options: SearchOptions = {},
  ): Promise<FullTextSearchResultWithFacts[]> {
    const searchFn = () => this.searchEngine.search(this.FACT_INDEX_NAME, query, options);

    const searchResults = this.performanceMonitor
      ? await this.performanceMonitor.monitorSearch(this.FACT_INDEX_NAME, query, searchFn)
      : await searchFn();

    // 将搜索结果映射为事实记录
    const results: FullTextSearchResultWithFacts[] = [];

    for (const result of searchResults) {
      const facts = this.findFactsFromSearchResult(result);
      const matchedContent = this.extractMatchedContent(result);

      results.push({
        searchResult: result,
        facts,
        matchedContent,
      });
    }

    return results;
  }

  /**
   * 搜索节点
   */
  async searchNodes(query: string, options: SearchOptions = {}): Promise<SearchResult[]> {
    const searchFn = () => this.searchEngine.search(this.NODE_INDEX_NAME, query, options);

    return this.performanceMonitor
      ? await this.performanceMonitor.monitorSearch(this.NODE_INDEX_NAME, query, searchFn)
      : await searchFn();
  }

  /**
   * 搜索边
   */
  async searchEdges(query: string, options: SearchOptions = {}): Promise<SearchResult[]> {
    const searchFn = () => this.searchEngine.search(this.EDGE_INDEX_NAME, query, options);

    return this.performanceMonitor
      ? await this.performanceMonitor.monitorSearch(this.EDGE_INDEX_NAME, query, searchFn)
      : await searchFn();
  }

  /**
   * 全局搜索（在所有索引中搜索）
   */
  async globalSearch(
    query: string,
    options: SearchOptions = {},
  ): Promise<{
    facts: FullTextSearchResultWithFacts[];
    nodes: SearchResult[];
    edges: SearchResult[];
  }> {
    const [facts, nodes, edges] = await Promise.all([
      this.searchFacts(query, options),
      this.searchNodes(query, options),
      this.searchEdges(query, options),
    ]);

    return { facts, nodes, edges };
  }

  /**
   * 为新添加的事实创建索引
   */
  async indexFact(fact: FactRecord): Promise<void> {
    const doc = this.factToDocument(fact);
    await this.searchEngine.indexDocument(this.FACT_INDEX_NAME, doc);

    // 索引节点属性
    if (fact.subjectProperties && Object.keys(fact.subjectProperties).length > 0) {
      const nodeDoc = this.nodeToDocument(fact.subject, fact.subjectProperties);
      await this.searchEngine.indexDocument(this.NODE_INDEX_NAME, nodeDoc);
    }

    if (fact.objectProperties && Object.keys(fact.objectProperties).length > 0) {
      const nodeDoc = this.nodeToDocument(fact.object, fact.objectProperties);
      await this.searchEngine.indexDocument(this.NODE_INDEX_NAME, nodeDoc);
    }

    // 索引边属性
    if (fact.edgeProperties && Object.keys(fact.edgeProperties).length > 0) {
      const edgeDoc = this.edgeToDocument(fact, fact.edgeProperties);
      await this.searchEngine.indexDocument(this.EDGE_INDEX_NAME, edgeDoc);
    }
  }

  /**
   * 删除事实的索引
   */
  async removeFact(fact: FactRecord): Promise<void> {
    const factId = `fact_${fact.subjectId}_${fact.predicateId}_${fact.objectId}`;
    await this.searchEngine.removeDocument(this.FACT_INDEX_NAME, factId);

    // 也可以删除相关的节点和边索引
    const nodeIdSubject = `node_${fact.subject}`;
    const nodeIdObject = `node_${fact.object}`;
    const edgeId = `edge_${fact.subjectId}_${fact.predicateId}_${fact.objectId}`;

    await Promise.all([
      this.searchEngine.removeDocument(this.NODE_INDEX_NAME, nodeIdSubject),
      this.searchEngine.removeDocument(this.NODE_INDEX_NAME, nodeIdObject),
      this.searchEngine.removeDocument(this.EDGE_INDEX_NAME, edgeId),
    ]);
  }

  /**
   * 获取搜索建议
   */
  async getSuggestions(
    prefix: string,
    count: number = 10,
  ): Promise<{
    facts: string[];
    nodes: string[];
    edges: string[];
  }> {
    const [facts, nodes, edges] = await Promise.all([
      this.searchEngine.suggest(this.FACT_INDEX_NAME, prefix, count),
      this.searchEngine.suggest(this.NODE_INDEX_NAME, prefix, count),
      this.searchEngine.suggest(this.EDGE_INDEX_NAME, prefix, count),
    ]);

    return { facts, nodes, edges };
  }

  /**
   * 获取索引统计信息
   */
  async getIndexStats(): Promise<{
    facts: IndexStats;
    nodes: IndexStats;
    edges: IndexStats;
  }> {
    const [facts, nodes, edges] = await Promise.all([
      this.searchEngine.getIndexStats(this.FACT_INDEX_NAME),
      this.searchEngine.getIndexStats(this.NODE_INDEX_NAME),
      this.searchEngine.getIndexStats(this.EDGE_INDEX_NAME),
    ]);

    return { facts, nodes, edges };
  }

  /**
   * 获取性能报告
   */
  getPerformanceReport(): { facts: unknown; nodes: unknown; edges: unknown } | null {
    if (!this.performanceMonitor) {
      return null;
    }

    return {
      facts: this.performanceMonitor.getPerformanceReport(this.FACT_INDEX_NAME),
      nodes: this.performanceMonitor.getPerformanceReport(this.NODE_INDEX_NAME),
      edges: this.performanceMonitor.getPerformanceReport(this.EDGE_INDEX_NAME),
    };
  }

  /**
   * 重建索引
   */
  async rebuildIndexes(): Promise<void> {
    // 删除现有索引
    await Promise.all([
      this.searchEngine.dropIndex(this.FACT_INDEX_NAME),
      this.searchEngine.dropIndex(this.NODE_INDEX_NAME),
      this.searchEngine.dropIndex(this.EDGE_INDEX_NAME),
    ]);

    // 重新初始化
    await this.initializeIndexes();
  }

  /**
   * 从搜索结果中查找关联的事实记录
   */
  private findFactsFromSearchResult(result: SearchResult): FactRecord[] {
    // 根据文档ID解析出事实信息
    if (result.docId.startsWith('fact_')) {
      const parts = result.docId.substring(5).split('_');
      if (parts.length === 3) {
        const [subjectId, predicateId, objectId] = parts.map(Number);

        // 从数据库中查找对应的事实
        const facts = this.db.listFacts();
        return facts.filter(
          (fact) =>
            fact.subjectId === subjectId &&
            fact.predicateId === predicateId &&
            fact.objectId === objectId,
        );
      }
    }

    return [];
  }

  /**
   * 提取匹配的内容
   */
  private extractMatchedContent(result: SearchResult): string[] {
    const content: string[] = [];

    if (result.highlights) {
      for (const highlight of result.highlights) {
        content.push(...highlight.fragments);
      }
    }

    return content;
  }
}

/**
 * NervusDB 全文搜索扩展工厂
 */
export class NervusDBFullTextExtensionFactory {
  /**
   * 为 NervusDB 实例创建全文搜索扩展
   */
  static async create(
    db: NervusDB,
    config?: FullTextExtensionConfig,
  ): Promise<NervusDBFullTextExtension> {
    const extension = new NervusDBFullTextExtension(db, config);
    // 等待初始化完成
    await new Promise((resolve) => setTimeout(resolve, 100)); // 简单延迟，实际应该等待初始化
    return extension;
  }

  /**
   * 创建高性能配置的扩展
   */
  static async createHighPerformance(db: NervusDB): Promise<NervusDBFullTextExtension> {
    return this.create(db, {
      enablePerformanceMonitoring: true,
      autoIndexTriples: true,
      batchSize: 2000,
    });
  }

  /**
   * 创建最小配置的扩展
   */
  static async createMinimal(db: NervusDB): Promise<NervusDBFullTextExtension> {
    return this.create(db, {
      autoIndexTriples: false,
      enablePerformanceMonitoring: false,
      batchSize: 500,
    });
  }
}
