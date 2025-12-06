/**
 * 查询处理引擎实现
 *
 * 提供布尔查询、模糊搜索、短语查询等完整的查询处理功能
 */

import type {
  Query,
  BooleanQuery,
  SearchOptions,
  SearchResult,
  SearchHighlight,
  InvertedIndex,
  DocumentCorpus,
  RelevanceScorer,
  TextAnalyzer,
  PostingList,
} from './types.js';

// 本文件内定义布尔与短语处理器

/**
 * 编辑距离计算（用于模糊搜索）
 */
export class EditDistanceCalculator {
  /**
   * 计算两个字符串的Levenshtein距离
   */
  static calculate(str1: string, str2: string): number {
    const m = str1.length;
    const n = str2.length;

    // 创建距离矩阵
    const dp: number[][] = Array.from({ length: m + 1 }, () => new Array<number>(n + 1).fill(0));

    // 初始化边界
    for (let i = 0; i <= m; i++) dp[i][0] = i;
    for (let j = 0; j <= n; j++) dp[0][j] = j;

    // 填充矩阵
    for (let i = 1; i <= m; i++) {
      for (let j = 1; j <= n; j++) {
        if (str1[i - 1] === str2[j - 1]) {
          dp[i][j] = dp[i - 1][j - 1];
        } else {
          dp[i][j] =
            1 +
            Math.min(
              dp[i - 1][j], // 删除
              dp[i][j - 1], // 插入
              dp[i - 1][j - 1], // 替换
            );
        }
      }
    }

    return dp[m][n];
  }

  /**
   * 检查两个字符串是否在指定编辑距离内
   */
  static isWithinDistance(str1: string, str2: string, maxDistance: number): boolean {
    // 快速检查：如果长度差超过最大距离，直接返回false
    if (Math.abs(str1.length - str2.length) > maxDistance) {
      return false;
    }

    return this.calculate(str1, str2) <= maxDistance;
  }
}

/**
 * 模糊搜索处理器
 */
export class FuzzySearchProcessor {
  private index: InvertedIndex;
  private maxEditDistance: number;

  constructor(index: InvertedIndex, maxEditDistance: number = 2) {
    this.index = index;
    this.maxEditDistance = maxEditDistance;
  }

  /**
   * 模糊搜索
   * @param term 搜索词
   * @param maxDistance 最大编辑距离
   */
  fuzzySearch(term: string, maxDistance: number = this.maxEditDistance): Set<string> {
    const results = new Set<string>();

    // 获取所有词汇并进行模糊匹配
    const allTerms = this.getAllTermsFromIndex();

    for (const indexTerm of allTerms) {
      if (EditDistanceCalculator.isWithinDistance(term, indexTerm, maxDistance)) {
        const postingList = this.index.getPostingList(indexTerm);
        if (postingList) {
          for (const entry of postingList.entries) {
            results.add(entry.docId);
          }
        }
      }
    }

    return results;
  }

  /**
   * 通配符搜索
   * @param pattern 通配符模式 (支持 * 和 ?)
   */
  wildcardSearch(pattern: string): Set<string> {
    const results = new Set<string>();
    const regex = this.convertWildcardToRegex(pattern);

    const allTerms = this.getAllTermsFromIndex();

    for (const term of allTerms) {
      if (regex.test(term)) {
        const postingList = this.index.getPostingList(term);
        if (postingList) {
          for (const entry of postingList.entries) {
            results.add(entry.docId);
          }
        }
      }
    }

    return results;
  }

  /**
   * 前缀搜索
   */
  prefixSearch(prefix: string): Set<string> {
    const results = new Set<string>();
    const allTerms = this.getAllTermsFromIndex();

    for (const term of allTerms) {
      if (term.startsWith(prefix)) {
        const postingList = this.index.getPostingList(term);
        if (postingList) {
          for (const entry of postingList.entries) {
            results.add(entry.docId);
          }
        }
      }
    }

    return results;
  }

  /**
   * 获取建议词汇（用于搜索提示）
   */
  getSuggestions(term: string, maxSuggestions: number = 10): string[] {
    const suggestions: Array<{ term: string; distance: number }> = [];
    const allTerms = this.getAllTermsFromIndex();

    for (const indexTerm of allTerms) {
      const distance = EditDistanceCalculator.calculate(term, indexTerm);
      if (distance <= 3) {
        // 最大距离阈值
        suggestions.push({ term: indexTerm, distance });
      }
    }

    // 按距离排序，距离越小排在前面
    suggestions.sort((a, b) => a.distance - b.distance);

    return suggestions.slice(0, maxSuggestions).map((s) => s.term);
  }

  /**
   * 将通配符模式转换为正则表达式
   */
  private convertWildcardToRegex(pattern: string): RegExp {
    const escaped = pattern
      .replace(/[.*+?^${}()|[\]\\]/g, '\\$&') // 转义特殊字符
      .replace(/\\\*/g, '.*') // * 转换为 .*
      .replace(/\\\?/g, '.'); // ? 转换为 .

    return new RegExp(`^${escaped}$`, 'i');
  }

  /**
   * 从索引中获取所有词汇（简化实现）
   * 实际应用中应该维护一个独立的词汇表
   */
  private getAllTermsFromIndex(): string[] {
    // 这是一个简化实现，实际中应该有更高效的方法
    const terms: string[] = [];
    // 假设我们有方法获取所有词汇
    return terms;
  }
}

/**
 * 布尔查询处理器（AND/OR/NOT）
 */
export class BooleanQueryProcessor {
  constructor(private index: InvertedIndex) {}

  processOR(terms: string[]): Set<string> {
    const result = new Set<string>();
    for (const term of terms) {
      const list = this.index.getPostingList(term);
      if (!list) continue;
      for (const e of list.entries) result.add(e.docId);
    }
    return result;
  }

  processAND(terms: string[]): Set<string> {
    if (terms.length === 0) return new Set();
    let current = this.processOR([terms[0]]);
    for (let i = 1; i < terms.length; i++) {
      const next = this.processOR([terms[i]]);
      current = new Set([...current].filter((id) => next.has(id)));
    }
    return current;
  }

  processNOT(includeTerms: string[], excludeTerms: string[]): Set<string> {
    const include = this.processOR(includeTerms);
    const exclude = this.processOR(excludeTerms);
    return new Set([...include].filter((id) => !exclude.has(id)));
  }
}

/**
 * 短语查询处理器（基于相邻位置匹配）
 */
export class PhraseQueryProcessor {
  constructor(private index: InvertedIndex) {}

  /**
   * 处理短语查询，支持 slop（词间允许的最大距离）
   * @param terms 词项列表
   * @param slop 允许词间的最大跨度（默认 0 表示严格连续）
   */
  processPhrase(terms: string[], slop: number = 0): Set<string> {
    if (terms.length === 0) return new Set();
    // 取出所有 posting 列表
    const lists = terms.map((t) => this.index.getPostingList(t)).filter(Boolean) as PostingList[];
    if (lists.length !== terms.length) return new Set();

    // 以第一个词为基准，检查其它词的位置是否在允许范围内
    const first = lists[0];
    const result = new Set<string>();

    for (const entry of first.entries) {
      const docId = entry.docId;
      const basePositions = new Set(entry.positions);
      let ok = true;

      for (let i = 1; i < lists.length && ok; i++) {
        const other = lists[i].entries.find((e) => e.docId === docId);
        if (!other) {
          ok = false;
          break;
        }
        // 检查是否存在位置在 [p+i, p+i+slop] 范围内
        let matched = false;
        for (const p of basePositions) {
          // 精确匹配：pos+i（连续）
          // 松弛匹配：pos+i 到 pos+i+slop（允许跨度）
          for (let offset = 0; offset <= slop; offset++) {
            if (other.positions.includes(p + i + offset)) {
              matched = true;
              break;
            }
          }
          if (matched) break;
        }
        if (!matched) ok = false;
      }

      if (ok) result.add(docId);
    }

    return result;
  }
}

/**
 * 查询解析器
 */
export class QueryParser {
  private analyzer: TextAnalyzer;

  constructor(analyzer: TextAnalyzer) {
    this.analyzer = analyzer;
  }

  /**
   * 解析查询字符串
   */
  parseQuery(queryString: string): Query[] {
    const queries: Query[] = [];

    // 检测查询类型并解析
    if (this.isBooleanQuery(queryString)) {
      queries.push(this.parseBooleanQuery(queryString));
    } else if (this.isPhraseQuery(queryString)) {
      queries.push(this.parsePhraseQuery(queryString));
    } else if (this.isWildcardQuery(queryString)) {
      queries.push(this.parseWildcardQuery(queryString));
    } else {
      // 默认为词匹配查询
      queries.push(this.parseTermQuery(queryString));
    }

    return queries;
  }

  /**
   * 检测是否为布尔查询
   */
  private isBooleanQuery(query: string): boolean {
    return /\b(AND|OR|NOT)\b/i.test(query);
  }

  /**
   * 检测是否为短语查询
   */
  private isPhraseQuery(query: string): boolean {
    return /^".*"$/.test(query.trim());
  }

  /**
   * 检测是否为通配符查询
   */
  private isWildcardQuery(query: string): boolean {
    return /[*?]/.test(query);
  }

  /**
   * 解析布尔查询
   */
  private parseBooleanQuery(query: string): Query {
    // 简化的布尔查询解析
    const normalizedQuery = query.toUpperCase();

    if (normalizedQuery.includes(' AND ')) {
      const parts = query.split(/\s+AND\s+/i);
      const booleanQuery: BooleanQuery = {
        operator: 'AND',
        queries: parts.map((part) => part.trim()),
      };

      return {
        type: 'boolean',
        value: booleanQuery,
      };
    } else if (normalizedQuery.includes(' OR ')) {
      const parts = query.split(/\s+OR\s+/i);
      const booleanQuery: BooleanQuery = {
        operator: 'OR',
        queries: parts.map((part) => part.trim()),
      };

      return {
        type: 'boolean',
        value: booleanQuery,
      };
    } else if (normalizedQuery.startsWith('NOT ')) {
      const term = query.substring(4).trim();
      const booleanQuery: BooleanQuery = {
        operator: 'NOT',
        queries: ['*', term], // 所有文档 - 包含该词的文档
      };

      return {
        type: 'boolean',
        value: booleanQuery,
      };
    }

    // 如果无法解析为布尔查询，作为普通词查询处理
    return this.parseTermQuery(query);
  }

  /**
   * 解析短语查询
   */
  private parsePhraseQuery(query: string): Query {
    const phrase = query.replace(/^"|"$/g, ''); // 移除引号

    return {
      type: 'phrase',
      value: phrase,
    };
  }

  /**
   * 解析通配符查询
   */
  private parseWildcardQuery(query: string): Query {
    return {
      type: 'wildcard',
      value: query,
    };
  }

  /**
   * 解析词匹配查询
   */
  private parseTermQuery(query: string): Query {
    return {
      type: 'term',
      value: query,
    };
  }
}

/**
 * 搜索结果高亮器
 */
export class SearchHighlighter {
  /**
   * 为搜索结果生成高亮
   */
  generateHighlights(
    docId: string,
    query: string[],
    corpus: DocumentCorpus,
    options: {
      fragmentSize?: number;
      maxFragments?: number;
      preTag?: string;
      postTag?: string;
    } = {},
  ): SearchHighlight[] {
    const {
      fragmentSize = 150,
      maxFragments = 3,
      preTag = '<mark>',
      postTag = '</mark>',
    } = options;

    const document = corpus.getDocument(docId);
    if (!document) return [];

    const highlights: SearchHighlight[] = [];

    // 为每个字段生成高亮
    for (const [fieldName, fieldContent] of document.fields) {
      const fragments = this.extractFragments(
        fieldContent,
        query,
        fragmentSize,
        maxFragments,
        preTag,
        postTag,
      );

      if (fragments.length > 0) {
        highlights.push({
          field: fieldName,
          fragments,
        });
      }
    }

    return highlights;
  }

  /**
   * 从字段内容中提取高亮片段
   */
  private extractFragments(
    content: string,
    query: string[],
    fragmentSize: number,
    maxFragments: number,
    preTag: string,
    postTag: string,
  ): string[] {
    const fragments: string[] = [];
    const lowercaseContent = content.toLowerCase();

    // 找到所有匹配位置
    const matches: Array<{ start: number; end: number; term: string }> = [];

    for (const term of query) {
      const lowercaseTerm = term.toLowerCase();
      let index = 0;

      while (true) {
        index = lowercaseContent.indexOf(lowercaseTerm, index);
        if (index === -1) break;

        matches.push({
          start: index,
          end: index + term.length,
          term: term,
        });

        index += term.length;
      }
    }

    // 按位置排序
    matches.sort((a, b) => a.start - b.start);

    // 生成片段
    const processedRanges = new Set<string>();

    for (const match of matches) {
      if (fragments.length >= maxFragments) break;

      const fragmentStart = Math.max(0, match.start - fragmentSize / 2);
      const fragmentEnd = Math.min(
        content.length,
        match.start + match.term.length + fragmentSize / 2,
      );

      const rangeKey = `${fragmentStart}-${fragmentEnd}`;
      if (processedRanges.has(rangeKey)) continue;

      processedRanges.add(rangeKey);

      let fragment = content.substring(fragmentStart, fragmentEnd);

      // 添加高亮标签
      for (const term of query) {
        const regex = new RegExp(`(${this.escapeRegex(term)})`, 'gi');
        fragment = fragment.replace(regex, `${preTag}$1${postTag}`);
      }

      // 添加省略号
      if (fragmentStart > 0) fragment = '...' + fragment;
      if (fragmentEnd < content.length) fragment = fragment + '...';

      fragments.push(fragment.trim());
    }

    return fragments;
  }

  /**
   * 转义正则表达式特殊字符
   */
  private escapeRegex(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }
}

/**
 * 主查询引擎
 */
export class FullTextQueryEngine {
  private index: InvertedIndex;
  private corpus: DocumentCorpus;
  private scorer: RelevanceScorer;
  private analyzer: TextAnalyzer;
  private parser: QueryParser;
  private highlighter: SearchHighlighter;
  private booleanProcessor: BooleanQueryProcessor;
  private phraseProcessor: PhraseQueryProcessor;
  private fuzzyProcessor: FuzzySearchProcessor;

  constructor(
    index: InvertedIndex,
    corpus: DocumentCorpus,
    scorer: RelevanceScorer,
    analyzer: TextAnalyzer,
  ) {
    this.index = index;
    this.corpus = corpus;
    this.scorer = scorer;
    this.analyzer = analyzer;
    this.parser = new QueryParser(analyzer);
    this.highlighter = new SearchHighlighter();
    this.booleanProcessor = new BooleanQueryProcessor(index);
    this.phraseProcessor = new PhraseQueryProcessor(index);
    this.fuzzyProcessor = new FuzzySearchProcessor(index);
  }

  /**
   * 执行搜索查询
   */
  search(queryString: string, options: SearchOptions = {}): SearchResult[] {
    const {
      fields = [],
      fuzzy = false,
      maxEditDistance = 2,
      minScore = 0,
      maxResults = 100,
      sortBy = 'relevance',
      highlight = false,
      highlightFragments = 3,
    } = options;

    // 解析查询
    const queries = this.parser.parseQuery(queryString);
    let candidateDocIds = new Set<string>();

    // 处理每个查询
    for (const query of queries) {
      const docIds = this.processQuery(query, fuzzy, maxEditDistance);

      if (candidateDocIds.size === 0) {
        candidateDocIds = docIds;
      } else {
        // 求交集（AND语义）
        candidateDocIds = new Set([...candidateDocIds].filter((id) => docIds.has(id)));
      }
    }

    // 分析查询词
    const queryTerms = this.analyzer.analyze(queryString).map((token) => token.value);

    // 计算评分
    const results: SearchResult[] = [];

    for (const docId of candidateDocIds) {
      const document = this.corpus.getDocument(docId);
      if (!document) continue;

      // 字段过滤
      if (fields.length > 0) {
        const hasMatchingField = fields.some((field) => document.fields.has(field));
        if (!hasMatchingField) continue;
      }

      // 计算相关性评分
      const score = this.scorer.calculateScore(queryTerms, document, this.corpus);

      // 评分过滤
      if (score < minScore) continue;

      // 生成高亮
      const highlights = highlight
        ? this.highlighter.generateHighlights(docId, queryTerms, this.corpus, {
            maxFragments: highlightFragments,
          })
        : [];

      results.push({
        docId,
        score,
        fields: Array.from(document.fields.keys()),
        highlights: highlights.length > 0 ? highlights : undefined,
      });
    }

    // 排序
    this.sortResults(results, sortBy);

    // 限制结果数量
    return results.slice(0, maxResults);
  }

  /**
   * 处理单个查询
   */
  private processQuery(query: Query, fuzzy: boolean, maxEditDistance: number): Set<string> {
    switch (query.type) {
      case 'term':
        return this.processTermQuery(query.value as string, fuzzy, maxEditDistance);

      case 'phrase':
        return this.processPhraseQuery(query.value as string);

      case 'boolean':
        return this.processBooleanQuery(query.value as BooleanQuery);

      case 'wildcard':
        return this.processWildcardQuery(query.value as string);

      case 'fuzzy':
        return this.fuzzyProcessor.fuzzySearch(query.value as string, maxEditDistance);

      default:
        return new Set();
    }
  }

  /**
   * 处理词匹配查询
   */
  private processTermQuery(term: string, fuzzy: boolean, maxEditDistance: number): Set<string> {
    if (fuzzy) {
      return this.fuzzyProcessor.fuzzySearch(term, maxEditDistance);
    }

    const tokens = this.analyzer.analyze(term);
    const termValues = tokens.map((token) => token.value);

    return this.booleanProcessor.processOR(termValues);
  }

  /**
   * 处理短语查询
   */
  private processPhraseQuery(phrase: string): Set<string> {
    const tokens = this.analyzer.analyze(phrase);
    const termValues = tokens.map((token) => token.value);

    return this.phraseProcessor.processPhrase(termValues, 0); // 精确短语匹配
  }

  /**
   * 处理布尔查询
   */
  private processBooleanQuery(booleanQuery: BooleanQuery): Set<string> {
    const termQueries: string[] = [];

    for (const subQuery of booleanQuery.queries) {
      if (typeof subQuery === 'string') {
        const tokens = this.analyzer.analyze(subQuery);
        termQueries.push(...tokens.map((token) => token.value));
      }
    }

    switch (booleanQuery.operator) {
      case 'AND':
        return this.booleanProcessor.processAND(termQueries);

      case 'OR':
        return this.booleanProcessor.processOR(termQueries);

      case 'NOT': {
        const [includeTerms, excludeTerms] = [
          termQueries.slice(0, termQueries.length / 2),
          termQueries.slice(termQueries.length / 2),
        ];
        return this.booleanProcessor.processNOT(includeTerms, excludeTerms);
      }

      default:
        return new Set();
    }
  }

  /**
   * 处理通配符查询
   */
  private processWildcardQuery(pattern: string): Set<string> {
    return this.fuzzyProcessor.wildcardSearch(pattern);
  }

  /**
   * 排序搜索结果
   */
  private sortResults(results: SearchResult[], sortBy: string): void {
    switch (sortBy) {
      case 'relevance':
        results.sort((a, b) => b.score - a.score);
        break;

      case 'date':
        // 需要从文档中获取时间戳进行排序
        results.sort((a, b) => {
          const docA = this.corpus.getDocument(a.docId);
          const docB = this.corpus.getDocument(b.docId);

          const timeA = docA?.timestamp?.getTime() || 0;
          const timeB = docB?.timestamp?.getTime() || 0;

          return timeB - timeA; // 最新的在前
        });
        break;

      case 'score':
        results.sort((a, b) => b.score - a.score);
        break;

      default:
        results.sort((a, b) => b.score - a.score);
    }
  }

  /**
   * 获取搜索建议
   */
  suggest(prefix: string, maxSuggestions: number = 10): Promise<string[]> {
    return Promise.resolve(this.fuzzyProcessor.getSuggestions(prefix, maxSuggestions));
  }
}
