/**
 * 全文搜索引擎入口
 *
 * 导出全文搜索功能的所有公共API和类型
 */

// 导出类型定义
export * from './types.js';

// 导出文本分析器
export { StandardAnalyzer, KeywordAnalyzer, NGramAnalyzer, AnalyzerFactory } from './analyzer.js';

// 导出倒排索引和文档语料库
export { MemoryInvertedIndex } from './invertedIndex.js';
export { MemoryDocumentCorpus } from './corpus.js';

// 导出相关性评分器
export {
  TFIDFScorer,
  BM25Scorer,
  FieldWeightedScorer,
  TimeDecayScorer,
  CompositeScorer,
  VectorSpaceScorer,
  ScorerFactory,
} from './scorer.js';

// 导出查询引擎
export {
  EditDistanceCalculator,
  FuzzySearchProcessor,
  BooleanQueryProcessor,
  PhraseQueryProcessor,
  QueryParser,
  SearchHighlighter,
  FullTextQueryEngine,
} from './query.js';

// 导出主搜索引擎
export {
  FullTextIndex,
  FullTextSearchEngine,
  FullTextSearchFactory,
  FullTextBatchProcessor,
  SearchPerformanceMonitor,
} from './engine.js';
