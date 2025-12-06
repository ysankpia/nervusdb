/**
 * NervusDB Extensions - Application Layer
 *
 * 应用层扩展，包含：
 * - 聚合查询（Aggregation）
 * - Cypher 支持（通过 Native 调用 Rust Core）
 * - 流式迭代器
 *
 * 注意：路径查找算法已完全迁移到 Rust Core：
 * - 使用 PathfindingPlugin 或 db.shortestPath() 调用 Native 实现
 * - TypeScript 参考实现在 _archive/ts-algorithms/ 目录
 */

// 聚合查询
export * from './query/aggregation.js';

// Cypher 支持 (通过 Native 调用 Rust Core)
export * from './query/cypher.js';

// 流式迭代器
export * from './query/iterator.js';
