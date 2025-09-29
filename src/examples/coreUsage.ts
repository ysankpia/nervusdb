/**
 * 示例：使用新的插件架构
 *
 * 展示了三种使用方式：
 * 1. 核心版本（最轻量）
 * 2. 选择性插件（推荐）
 * 3. 完整版本（向后兼容）
 */

import {
  CoreSynapseDB,
  ExtendedSynapseDB,
  SynapseDB,
  PathfindingPlugin,
  AggregationPlugin,
} from '../index.js';

// ======================
// 方式1：核心版本（最轻量）
// ======================
export async function coreExample() {
  const db = await CoreSynapseDB.open(':memory:');

  // 只有基本的CRUD功能
  db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
  const facts = db.listFacts();
  const query = db.find({ predicate: 'knows' });
  const result = query.all();

  await db.close();
  console.log('Core example:', facts.length, 'facts, query hits', result.length);
}

// ======================
// 方式2：选择性插件（推荐）
// ======================
export async function selectivePluginExample() {
  // 注意：插件现在自动加载，无需手动指定
  const db = await ExtendedSynapseDB.open(':memory:');

  // 基本功能
  db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
  db.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });

  // 使用路径查询插件
  const pathPlugin = db.plugin<PathfindingPlugin>('pathfinding');
  if (pathPlugin) {
    const path = pathPlugin.shortestPath('Alice', 'Charlie');
    console.log('Shortest path:', path?.length, 'edges');
  }

  // 使用聚合插件
  const aggPlugin = db.plugin<AggregationPlugin>('aggregation');
  if (aggPlugin) {
    const stats = aggPlugin.getStatsSummary();
    console.log('Stats:', stats);
  }

  await db.close();
}

// ======================
// 方式3：完整版本（向后兼容）
// ======================
export async function compatibilityExample() {
  // 包含所有插件的完整版本
  const db = await SynapseDB.open(':memory:');

  // 所有原有API都可以使用
  db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
  db.addFact({ subject: 'Bob', predicate: 'knows', object: 'Charlie' });

  // 路径查询
  const path = db.shortestPath('Alice', 'Charlie');
  console.log('Path found:', path !== null);

  // 聚合查询
  const aggResults = db
    .aggregate()
    .match({ predicate: 'knows' })
    .groupBy(['predicate'])
    .count('edges')
    .execute();

  // Cypher查询
  const cypherResults = await db.cypher('MATCH (a)-[:knows]->(b) RETURN a,b');
  console.log('Aggregation results:', aggResults);
  console.log('Cypher results:', cypherResults.records.length);

  await db.close();
}

// ======================
// 性能对比
// ======================
export async function performanceComparison() {
  const iterations = 1000;

  // 测试核心版本
  const startCore = performance.now();
  const coreDb = await CoreSynapseDB.open(':memory:');
  for (let i = 0; i < iterations; i++) {
    coreDb.addFact({ subject: `node${i}`, predicate: 'connects', object: `node${i + 1}` });
  }
  await coreDb.close();
  const coreTime = performance.now() - startCore;

  // 测试完整版本
  const startFull = performance.now();
  const fullDb = await SynapseDB.open(':memory:');
  for (let i = 0; i < iterations; i++) {
    fullDb.addFact({ subject: `node${i}`, predicate: 'connects', object: `node${i + 1}` });
  }
  await fullDb.close();
  const fullTime = performance.now() - startFull;

  console.log(`Performance comparison for ${iterations} operations:`);
  console.log(`Core version: ${coreTime.toFixed(2)}ms`);
  console.log(`Full version: ${fullTime.toFixed(2)}ms`);
  console.log(`Overhead: ${(((fullTime - coreTime) / coreTime) * 100).toFixed(1)}%`);
}

// 如果直接运行此文件
if (import.meta.url === `file://${process.argv[1]}`) {
  (async () => {
    console.log('=== Core Usage Examples ===');
    await coreExample();
    await selectivePluginExample();
    await compatibilityExample();
    await performanceComparison();
  })().catch(console.error);
}
