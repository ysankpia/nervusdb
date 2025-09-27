/**
 * 全文搜索引擎类型定义
 *
 * 提供全文搜索功能的核心数据结构和接口定义
 */

// Token 类型定义
export interface Token {
  /** 词元文本 */
  value: string;
  /** 词元类型 */
  type: 'word' | 'ngram' | 'phrase';
  /** 原始位置 */
  position: number;
  /** 词元长度 */
  length: number;
}

// 文档表示
export interface Document {
  /** 文档唯一标识 */
  id: string;
  /** 文档字段内容 */
  fields: Map<string, string>;
  /** 分析后的词元 */
  tokens: Token[];
  /** TF-IDF 向量 */
  vector?: number[];
  /** 文档创建时间 */
  timestamp?: Date;
}

// 倒排索引条目
export interface PostingEntry {
  /** 文档ID */
  docId: string;
  /** 词频 */
  frequency: number;
  /** 词元位置列表 */
  positions: number[];
  /** 字段名称 */
  field: string;
}

// 倒排索引列表
export interface PostingList {
  /** 词元 */
  term: string;
  /** 包含该词元的文档列表 */
  entries: PostingEntry[];
  /** 文档频率（包含该词元的文档数） */
  documentFrequency: number;
}

// 搜索结果
export interface SearchResult {
  /** 文档ID */
  docId: string;
  /** 相关性评分 */
  score: number;
  /** 匹配字段 */
  fields: string[];
  /** 匹配的文本片段 */
  highlights?: SearchHighlight[];
}

// 搜索高亮
export interface SearchHighlight {
  /** 字段名 */
  field: string;
  /** 高亮片段 */
  fragments: string[];
}

// 搜索选项
export interface SearchOptions {
  /** 搜索字段列表 */
  fields?: string[];
  /** 是否启用模糊搜索 */
  fuzzy?: boolean;
  /** 模糊搜索最大编辑距离 */
  maxEditDistance?: number;
  /** 最小相关性评分 */
  minScore?: number;
  /** 最大结果数量 */
  maxResults?: number;
  /** 排序方式 */
  sortBy?: 'relevance' | 'date' | 'score';
  /** 是否启用高亮 */
  highlight?: boolean;
  /** 高亮片段数量 */
  highlightFragments?: number;
}

// 全文索引配置
export interface FullTextConfig {
  /** 索引字段列表 */
  fields: string[];
  /** 语言设置 */
  language: 'zh' | 'en' | 'auto';
  /** 分析器类型 */
  analyzer: 'standard' | 'keyword' | 'ngram';
  /** 是否启用词干提取 */
  stemming: boolean;
  /** 是否过滤停用词 */
  stopWords: boolean;
  /** N-gram 大小 */
  ngramSize?: number;
}

// 文本分析器接口
export interface TextAnalyzer {
  /** 分析文本，返回词元列表 */
  analyze(text: string, language?: string): Token[];

  /** 标准化文本 */
  normalize(text: string): string;

  /** 生成N-gram */
  generateNGrams(tokens: string[], n: number): string[];
}

// 倒排索引接口
export interface InvertedIndex {
  /** 添加文档到索引 */
  addDocument(doc: Document): void;

  /** 从索引中删除文档 */
  removeDocument(docId: string): void;

  /** 更新文档索引 */
  updateDocument(doc: Document): void;

  /** 搜索词元 */
  search(terms: string[]): Map<string, number>;

  /** 获取词元的倒排列表 */
  getPostingList(term: string): PostingList | undefined;

  /** 获取文档总数 */
  getDocumentCount(): number;
}

// 相关性评分器接口
export interface RelevanceScorer {
  /** 计算查询与文档的相关性评分 */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number;

  /** 计算TF-IDF评分 */
  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number;

  /** 计算BM25评分 */
  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    k1?: number,
    b?: number,
  ): number;
}

// 文档语料库接口
export interface DocumentCorpus {
  /** 总文档数 */
  totalDocuments: number;

  /** 平均文档长度 */
  averageDocumentLength: number;

  /** 获取包含指定词元的文档 */
  getDocumentsContaining(term: string): Document[];

  /** 获取文档 */
  getDocument(docId: string): Document | undefined;
}

// 布尔查询类型
export type BooleanOperator = 'AND' | 'OR' | 'NOT';

export interface BooleanQuery {
  operator: BooleanOperator;
  queries: (string | BooleanQuery)[];
}

// 查询类型
export type QueryType =
  | 'term' // 精确词匹配
  | 'phrase' // 短语查询
  | 'wildcard' // 通配符查询
  | 'fuzzy' // 模糊查询
  | 'boolean' // 布尔查询
  | 'range'; // 范围查询

export interface Query {
  type: QueryType;
  value: string | BooleanQuery;
  field?: string;
  boost?: number;
}

// 全文搜索引擎主接口
export interface FullTextSearchEngine {
  /** 创建全文索引 */
  createIndex(name: string, config: FullTextConfig): Promise<void>;

  /** 删除全文索引 */
  dropIndex(name: string): Promise<void>;

  /** 添加文档到索引 */
  indexDocument(indexName: string, doc: Document): Promise<void>;

  /** 从索引中删除文档 */
  removeDocument(indexName: string, docId: string): Promise<void>;

  /** 执行搜索 */
  search(
    indexName: string,
    query: string | Query,
    options?: SearchOptions,
  ): Promise<SearchResult[]>;

  /** 搜索建议 */
  suggest(indexName: string, prefix: string, count: number): Promise<string[]>;

  /** 获取索引统计信息 */
  getIndexStats(indexName: string): Promise<IndexStats>;
}

// 索引统计信息
export interface IndexStats {
  /** 索引名称 */
  name: string;
  /** 文档总数 */
  documentCount: number;
  /** 唯一词元数 */
  uniqueTerms: number;
  /** 索引大小（字节） */
  indexSize: number;
  /** 最后更新时间 */
  lastUpdated: Date;
}
