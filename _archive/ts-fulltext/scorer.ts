/**
 * 相关性评分算法实现
 *
 * 提供TF-IDF、BM25等经典相关性评分算法
 */

import type { Document, DocumentCorpus, RelevanceScorer } from './types.js';

/**
 * TF-IDF相关性评分器
 */
export class TFIDFScorer implements RelevanceScorer {
  /**
   * 计算查询与文档的相关性评分
   */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number {
    let score = 0;

    for (const term of query) {
      const tfIdf = this.calculateTFIDF(term, document, corpus);
      score += tfIdf;
    }

    // 文档长度归一化
    const docLength = document.tokens.length;
    if (docLength > 0) {
      score /= Math.sqrt(docLength);
    }

    return score;
  }

  /**
   * 计算单个词元的TF-IDF评分
   */
  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number {
    const tf = this.calculateTF(term, document);
    const idf = this.calculateIDF(term, corpus);

    return tf * idf;
  }

  /**
   * 计算词频 (Term Frequency)
   */
  private calculateTF(term: string, document: Document): number {
    const termCount = document.tokens.filter((token) => token.value === term).length;
    const totalTokens = document.tokens.length;

    if (totalTokens === 0) return 0;

    // 使用对数归一化的TF
    return termCount > 0 ? 1 + Math.log(termCount) : 0;
  }

  /**
   * 计算逆文档频率 (Inverse Document Frequency)
   */
  private calculateIDF(term: string, corpus: DocumentCorpus): number {
    const docsContainingTerm = corpus.getDocumentsContaining(term).length;
    const totalDocs = corpus.totalDocuments;

    if (docsContainingTerm === 0) return 0;

    // 平滑IDF以避免除零
    return Math.log(totalDocs / (1 + docsContainingTerm));
  }

  /**
   * BM25评分（存根实现，实际由BM25Scorer处理）
   */
  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    _k1?: number,
    _b?: number,
  ): number {
    void _k1;
    void _b;
    // 使用TF-IDF作为fallback
    return this.calculateScore(query, document, corpus);
  }
}

/**
 * BM25相关性评分器
 * BM25是目前最优秀的概率检索模型之一
 */
export class BM25Scorer implements RelevanceScorer {
  private k1: number; // 控制词频饱和度
  private b: number; // 控制文档长度归一化程度

  constructor(k1: number = 1.2, b: number = 0.75) {
    this.k1 = k1;
    this.b = b;
  }

  /**
   * 计算BM25评分
   */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number {
    return this.calculateBM25(query, document, corpus, this.k1, this.b);
  }

  /**
   * 计算BM25评分
   */
  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    k1: number = this.k1,
    b: number = this.b,
  ): number {
    let score = 0;
    const docLength = document.tokens.length;
    const avgDocLength = corpus.averageDocumentLength;

    for (const term of query) {
      const tf = this.getTermFrequency(term, document);
      const idf = this.calculateIDF(term, corpus);

      // BM25公式
      const numerator = tf * (k1 + 1);
      const denominator = tf + k1 * (1 - b + b * (docLength / avgDocLength));

      score += idf * (numerator / denominator);
    }

    return score;
  }

  /**
   * 获取词频
   */
  private getTermFrequency(term: string, document: Document): number {
    return document.tokens.filter((token) => token.value === term).length;
  }

  /**
   * 计算IDF
   */
  private calculateIDF(term: string, corpus: DocumentCorpus): number {
    const docsContainingTerm = corpus.getDocumentsContaining(term).length;
    const totalDocs = corpus.totalDocuments;

    if (docsContainingTerm === 0) return 0;

    // BM25 IDF公式
    return Math.log((totalDocs - docsContainingTerm + 0.5) / (docsContainingTerm + 0.5));
  }

  /**
   * TF-IDF评分（存根实现）
   */
  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number {
    // BM25不使用传统TF-IDF，返回BM25单词得分
    const tf = this.getTermFrequency(term, document);
    const idf = this.calculateIDF(term, corpus);
    const docLength = document.tokens.length;
    const avgDocLength = corpus.averageDocumentLength;

    const numerator = tf * (this.k1 + 1);
    const denominator = tf + this.k1 * (1 - this.b + this.b * (docLength / avgDocLength));

    return idf * (numerator / denominator);
  }
}

/**
 * 字段权重评分器
 * 对不同字段赋予不同权重
 */
export class FieldWeightedScorer implements RelevanceScorer {
  private baseScorer: RelevanceScorer;
  private fieldWeights: Map<string, number>;

  constructor(baseScorer: RelevanceScorer, fieldWeights: Map<string, number> = new Map()) {
    this.baseScorer = baseScorer;
    this.fieldWeights = fieldWeights;

    // 默认权重
    if (this.fieldWeights.size === 0) {
      this.fieldWeights.set('title', 3.0);
      this.fieldWeights.set('content', 1.0);
      this.fieldWeights.set('tags', 2.0);
      this.fieldWeights.set('description', 1.5);
    }
  }

  /**
   * 计算带字段权重的评分
   */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number {
    let totalScore = 0;

    // 为每个字段计算评分并应用权重
    for (const [fieldName, fieldContent] of document.fields) {
      const fieldDoc = this.createFieldDocument(document, fieldName, fieldContent);
      const fieldScore = this.baseScorer.calculateScore(query, fieldDoc, corpus);
      const weight = this.fieldWeights.get(fieldName) || 1.0;

      totalScore += fieldScore * weight;
    }

    return totalScore;
  }

  /**
   * 创建字段专用的文档对象
   */
  private createFieldDocument(
    document: Document,
    fieldName: string,
    fieldContent: string,
  ): Document {
    // 只包含该字段的tokens
    const fieldTokens = document.tokens.filter((token) =>
      fieldContent.toLowerCase().includes(token.value.toLowerCase()),
    );

    return {
      ...document,
      tokens: fieldTokens,
      fields: new Map([[fieldName, fieldContent]]),
    };
  }

  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number {
    return this.baseScorer.calculateTFIDF(term, document, corpus);
  }

  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    k1?: number,
    b?: number,
  ): number {
    return this.baseScorer.calculateBM25(query, document, corpus, k1, b);
  }
}

/**
 * 时间衰减评分器
 * 根据文档的时间新鲜度调整评分
 */
export class TimeDecayScorer implements RelevanceScorer {
  private baseScorer: RelevanceScorer;
  private decayRate: number; // 衰减率
  private timeUnit: number; // 时间单位（毫秒）

  constructor(
    baseScorer: RelevanceScorer,
    decayRate: number = 0.1,
    timeUnit: number = 24 * 60 * 60 * 1000, // 1天
  ) {
    this.baseScorer = baseScorer;
    this.decayRate = decayRate;
    this.timeUnit = timeUnit;
  }

  /**
   * 计算带时间衰减的评分
   */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number {
    const baseScore = this.baseScorer.calculateScore(query, document, corpus);
    const timeBoost = this.calculateTimeBoost(document);

    return baseScore * timeBoost;
  }

  /**
   * 计算时间提升因子
   */
  private calculateTimeBoost(document: Document): number {
    if (!document.timestamp) return 1.0;

    const now = new Date().getTime();
    const docTime = document.timestamp.getTime();
    const timeDiff = now - docTime;
    const timeUnits = timeDiff / this.timeUnit;

    // 指数衰减函数
    return Math.exp(-this.decayRate * timeUnits);
  }

  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number {
    return this.baseScorer.calculateTFIDF(term, document, corpus);
  }

  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    k1?: number,
    b?: number,
  ): number {
    return this.baseScorer.calculateBM25(query, document, corpus, k1, b);
  }
}

/**
 * 组合评分器
 * 支持多个评分器的加权组合
 */
export class CompositeScorer implements RelevanceScorer {
  private scorers: Array<{ scorer: RelevanceScorer; weight: number }>;

  constructor(scorers: Array<{ scorer: RelevanceScorer; weight: number }> = []) {
    this.scorers = scorers;

    // 权重归一化
    this.normalizeWeights();
  }

  /**
   * 添加评分器
   */
  addScorer(scorer: RelevanceScorer, weight: number): void {
    this.scorers.push({ scorer, weight });
    this.normalizeWeights();
  }

  /**
   * 计算组合评分
   */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number {
    let totalScore = 0;

    for (const { scorer, weight } of this.scorers) {
      const score = scorer.calculateScore(query, document, corpus);
      totalScore += score * weight;
    }

    return totalScore;
  }

  /**
   * 权重归一化
   */
  private normalizeWeights(): void {
    const totalWeight = this.scorers.reduce((sum, { weight }) => sum + weight, 0);

    if (totalWeight > 0) {
      for (const item of this.scorers) {
        item.weight = item.weight / totalWeight;
      }
    }
  }

  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number {
    if (this.scorers.length === 0) return 0;

    // 使用第一个评分器的TF-IDF
    return this.scorers[0].scorer.calculateTFIDF(term, document, corpus);
  }

  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    k1?: number,
    b?: number,
  ): number {
    if (this.scorers.length === 0) return 0;

    // 使用第一个评分器的BM25
    return this.scorers[0].scorer.calculateBM25(query, document, corpus, k1, b);
  }
}

/**
 * 向量空间模型评分器
 * 使用余弦相似度计算文档相关性
 */
export class VectorSpaceScorer implements RelevanceScorer {
  /**
   * 计算向量空间模型评分
   */
  calculateScore(query: string[], document: Document, corpus: DocumentCorpus): number {
    const queryVector = this.buildQueryVector(query, corpus);
    const docVector = this.buildDocumentVector(document, corpus);

    return this.cosineSimilarity(queryVector, docVector);
  }

  /**
   * 构建查询向量
   */
  private buildQueryVector(query: string[], corpus: DocumentCorpus): Map<string, number> {
    const vector = new Map<string, number>();
    const queryTerms = [...new Set(query)]; // 去重

    for (const term of queryTerms) {
      const tf = query.filter((t) => t === term).length / query.length;
      const idf = this.calculateIDF(term, corpus);
      vector.set(term, tf * idf);
    }

    return vector;
  }

  /**
   * 构建文档向量
   */
  private buildDocumentVector(document: Document, corpus: DocumentCorpus): Map<string, number> {
    const vector = new Map<string, number>();
    const uniqueTerms = [...new Set(document.tokens.map((t) => t.value))];

    for (const term of uniqueTerms) {
      const tf = document.tokens.filter((t) => t.value === term).length / document.tokens.length;
      const idf = this.calculateIDF(term, corpus);
      vector.set(term, tf * idf);
    }

    return vector;
  }

  /**
   * 计算余弦相似度
   */
  private cosineSimilarity(vectorA: Map<string, number>, vectorB: Map<string, number>): number {
    const commonTerms = new Set([...Array.from(vectorA.keys()), ...Array.from(vectorB.keys())]);

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (const term of commonTerms) {
      const valueA = vectorA.get(term) || 0;
      const valueB = vectorB.get(term) || 0;

      dotProduct += valueA * valueB;
      normA += valueA * valueA;
      normB += valueB * valueB;
    }

    const denominator = Math.sqrt(normA) * Math.sqrt(normB);
    return denominator === 0 ? 0 : dotProduct / denominator;
  }

  /**
   * 计算IDF
   */
  private calculateIDF(term: string, corpus: DocumentCorpus): number {
    const docsContainingTerm = corpus.getDocumentsContaining(term).length;
    const totalDocs = corpus.totalDocuments;

    if (docsContainingTerm === 0) return 0;

    return Math.log(totalDocs / docsContainingTerm);
  }

  calculateTFIDF(term: string, document: Document, corpus: DocumentCorpus): number {
    const tf = document.tokens.filter((t) => t.value === term).length / document.tokens.length;
    const idf = this.calculateIDF(term, corpus);
    return tf * idf;
  }

  calculateBM25(
    query: string[],
    document: Document,
    corpus: DocumentCorpus,
    _k1?: number,
    _b?: number,
  ): number {
    void _k1;
    void _b;
    // 向量空间模型不使用BM25，返回向量相似度
    return this.calculateScore(query, document, corpus);
  }
}

/**
 * 评分器工厂
 */
export class ScorerFactory {
  /**
   * 创建评分器实例
   */
  static createScorer(
    type: 'tfidf' | 'bm25' | 'vector' | 'composite',
    options?: { k1?: number; b?: number; scorers?: { scorer: RelevanceScorer; weight: number }[] },
  ): RelevanceScorer {
    switch (type) {
      case 'tfidf':
        return new TFIDFScorer();

      case 'bm25':
        return new BM25Scorer(options?.k1, options?.b);

      case 'vector':
        return new VectorSpaceScorer();

      case 'composite':
        return new CompositeScorer(options?.scorers || []);

      default:
        // 类型已穷尽
        throw new Error('Unknown scorer type');
    }
  }

  /**
   * 创建默认的生产级评分器
   */
  static createDefaultScorer(): RelevanceScorer {
    // 使用BM25作为基础评分器
    const bm25Scorer = new BM25Scorer(1.2, 0.75);

    // 添加字段权重
    const fieldWeights = new Map([
      ['title', 3.0],
      ['content', 1.0],
      ['tags', 2.0],
      ['description', 1.5],
    ]);

    const fieldWeightedScorer = new FieldWeightedScorer(bm25Scorer, fieldWeights);

    // 添加时间衰减
    return new TimeDecayScorer(fieldWeightedScorer, 0.05);
  }
}
