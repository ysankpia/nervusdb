#!/usr/bin/env node
/**
 * 综合性能基准测试套件 - v1.1 里程碑
 * 涵盖插入、查询、聚合、路径等核心功能的性能测试
 *
 * 用法:
 *   node benchmarks/comprehensive.mjs
 *   node --expose-gc benchmarks/comprehensive.mjs  # 启用GC测量
 */

import { NervusDB } from '../dist/index.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtemp, rm } from 'node:fs/promises';
import {
  BenchmarkSuite,
  Benchmark,
  BenchmarkRunner,
  PerformanceValidator
} from './framework.mjs';

// 测试配置
const CONFIG = {
  SMALL_DATASET: 1000,
  MEDIUM_DATASET: 10000,
  LARGE_DATASET: 50000,
  MEMORY_LIMIT_MB: 200,
  TIME_LIMIT_FAST_MS: 50,
  TIME_LIMIT_MEDIUM_MS: 500,
  TIME_LIMIT_SLOW_MS: 2000,
};

/**
 * 创建基础插入性能测试套件
 */
function createInsertionSuite() {
  const suite = new BenchmarkSuite(
    '数据插入性能测试',
    '测试不同规模数据的插入性能和内存使用'
  );

  let db, tempDir;

  suite.setGlobalSetup(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapsedb-bench-'));
    db = await NervusDB.open(join(tempDir, 'insertion.synapsedb'), {
      pageSize: 2000,
      compression: { codec: 'brotli', level: 4 }
    });
  });

  suite.setGlobalTeardown(async () => {
    if (db) await db.close();
    if (tempDir) await rm(tempDir, { recursive: true, force: true });
  });

  // 小规模批量插入
  suite.addBenchmark(new Benchmark(
    '小规模批量插入',
    `插入 ${CONFIG.SMALL_DATASET.toLocaleString()} 条三元组`,
    () => Promise.resolve(),
    async () => {
      const count = CONFIG.SMALL_DATASET;
      for (let i = 0; i < count; i++) {
        db.addFact({
          subject: `user_${i}`,
          predicate: 'follows',
          object: `user_${(i + 1) % count}`
        });
      }
      await db.flush();
      return count;
    },
    () => Promise.resolve(),
    PerformanceValidator.resultCount(CONFIG.SMALL_DATASET)
  ));

  // 中规模批量插入
  suite.addBenchmark(new Benchmark(
    '中规模批量插入',
    `插入 ${CONFIG.MEDIUM_DATASET.toLocaleString()} 条三元组`,
    () => Promise.resolve(),
    async () => {
      const count = CONFIG.MEDIUM_DATASET;
      for (let i = 0; i < count; i++) {
        db.addFact({
          subject: `item_${i % 1000}`,
          predicate: 'related_to',
          object: `category_${Math.floor(i / 100)}`
        });
      }
      await db.flush();
      return count;
    },
    () => Promise.resolve(),
    PerformanceValidator.resultCount(CONFIG.MEDIUM_DATASET)
  ));

  // 带属性的插入
  suite.addBenchmark(new Benchmark(
    '带属性插入',
    '插入带有节点和边属性的三元组',
    () => Promise.resolve(),
    async () => {
      const count = CONFIG.SMALL_DATASET;
      for (let i = 0; i < count; i++) {
        db.addFact(
          { subject: `person_${i}`, predicate: 'knows', object: `person_${(i + 1) % count}` },
          {
            subjectProperties: { name: `Person ${i}`, age: 20 + (i % 60) },
            objectProperties: { name: `Person ${(i + 1) % count}`, age: 20 + ((i + 1) % 60) },
            edgeProperties: { since: 2020 + (i % 5), strength: Math.random() }
          }
        );
      }
      await db.flush();
      return count;
    },
    () => Promise.resolve(),
    PerformanceValidator.resultCount(CONFIG.SMALL_DATASET)
  ));

  return suite;
}

/**
 * 创建查询性能测试套件
 */
function createQuerySuite() {
  const suite = new BenchmarkSuite(
    '查询性能测试',
    '测试不同类型查询的性能和流式处理'
  );

  let db, tempDir;
  const dataSize = CONFIG.MEDIUM_DATASET;

  suite.setGlobalSetup(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapsedb-bench-'));
    db = await NervusDB.open(join(tempDir, 'query.synapsedb'));

    // 准备测试数据
    console.log('  🔧 准备查询测试数据...');
    for (let i = 0; i < dataSize; i++) {
      if (i % 1000 === 0) process.stdout.write(`.`);

      // 创建星型图结构
      db.addFact({ subject: 'hub', predicate: 'connects', object: `node_${i}` });

      // 创建链式结构
      if (i < dataSize - 1) {
        db.addFact({ subject: `chain_${i}`, predicate: 'next', object: `chain_${i + 1}` });
      }

      // 创建属性丰富的节点
      if (i % 10 === 0) {
        db.addFact(
          { subject: `rich_${i}`, predicate: 'has_data', object: `value_${i}` },
          {
            subjectProperties: {
              type: 'rich_node',
              score: Math.random() * 100,
              category: `cat_${i % 5}`,
              active: i % 2 === 0
            }
          }
        );
      }
    }
    await db.flush();
    console.log('\n');
  });

  suite.setGlobalTeardown(async () => {
    if (db) await db.close();
    if (tempDir) await rm(tempDir, { recursive: true, force: true });
  });

  // 精确查询
  suite.addBenchmark(new Benchmark(
    '精确三元组查询',
    '查询具体的主语-谓语-宾语组合',
    () => Promise.resolve(),
    async () => {
      const results = db.find({
        subject: 'hub',
        predicate: 'connects',
        object: 'node_100'
      }).all();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length >= 0
  ));

  // 模式查询 - 星型展开
  suite.addBenchmark(new Benchmark(
    '星型模式查询',
    '从中心节点展开查找所有连接',
    () => Promise.resolve(),
    async () => {
      const results = db.find({ subject: 'hub', predicate: 'connects' }).all();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length === dataSize
  ));

  // 流式查询
  suite.addBenchmark(new Benchmark(
    '大结果集流式查询',
    '使用异步迭代器处理大结果集',
    () => Promise.resolve(),
    async () => {
      let count = 0;
      for await (const fact of db.find({ predicate: 'connects' })) {
        count++;
      }
      return count;
    },
    () => Promise.resolve(),
    PerformanceValidator.resultCount(dataSize)
  ));

  // 链式查询
  suite.addBenchmark(new Benchmark(
    '链式联想查询',
    '通过follow进行多跳查询',
    () => Promise.resolve(),
    async () => {
      const results = db.find({ subject: 'chain_0' })
        .follow('next')
        .follow('next')
        .follow('next')
        .all();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length >= 0
  ));

  // 属性过滤查询
  suite.addBenchmark(new Benchmark(
    '属性过滤查询',
    '基于节点属性进行过滤查询',
    () => Promise.resolve(),
    async () => {
      const results = db.find({ predicate: 'has_data' })
        .where(r => r.subjectProperties?.score > 80)
        .all();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length >= 0
  ));

  return suite;
}

/**
 * 创建路径和图算法性能测试套件
 */
function createPathSuite() {
  const suite = new BenchmarkSuite(
    '图算法性能测试',
    '测试最短路径、变长路径等图算法性能'
  );

  let db, tempDir;
  const nodeCount = 200; // 创建中等规模图

  suite.setGlobalSetup(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapsedb-bench-'));
    db = await NervusDB.open(join(tempDir, 'path.synapsedb'));

    // 创建测试图：网格+随机边
    console.log('  🔧 构建测试图结构...');
    const gridSize = Math.floor(Math.sqrt(nodeCount));

    // 网格连接
    for (let i = 0; i < gridSize; i++) {
      for (let j = 0; j < gridSize; j++) {
        const nodeId = i * gridSize + j;
        const nodeName = `n_${nodeId}`;

        // 水平连接
        if (j < gridSize - 1) {
          const rightNode = `n_${i * gridSize + j + 1}`;
          db.addFact(
            { subject: nodeName, predicate: 'connected', object: rightNode },
            { edgeProperties: { weight: 1 + Math.random() } }
          );
        }

        // 垂直连接
        if (i < gridSize - 1) {
          const downNode = `n_${(i + 1) * gridSize + j}`;
          db.addFact(
            { subject: nodeName, predicate: 'connected', object: downNode },
            { edgeProperties: { weight: 1 + Math.random() } }
          );
        }
      }
    }

    // 添加一些随机长距离连接
    for (let i = 0; i < nodeCount / 10; i++) {
      const from = `n_${Math.floor(Math.random() * nodeCount)}`;
      const to = `n_${Math.floor(Math.random() * nodeCount)}`;
      if (from !== to) {
        db.addFact(
          { subject: from, predicate: 'shortcut', object: to },
          { edgeProperties: { weight: 0.5 + Math.random() * 2 } }
        );
      }
    }

    await db.flush();
  });

  suite.setGlobalTeardown(async () => {
    if (db) await db.close();
    if (tempDir) await rm(tempDir, { recursive: true, force: true });
  });

  // 单向BFS最短路径
  suite.addBenchmark(new Benchmark(
    '单向BFS最短路径',
    '使用标准BFS算法查找最短路径',
    () => Promise.resolve(),
    async () => {
      const path = db.shortestPath('n_0', `n_${nodeCount - 1}`, {
        predicates: ['connected', 'shortcut'],
        maxHops: 20
      });
      return path;
    },
    () => Promise.resolve(),
    (result) => result === null || (Array.isArray(result) && result.length >= 0)
  ));

  // 双向BFS最短路径（优化版）
  suite.addBenchmark(new Benchmark(
    '双向BFS最短路径',
    '使用优化的双向BFS算法',
    () => Promise.resolve(),
    async () => {
      const path = db.shortestPathBidirectional('n_0', `n_${nodeCount - 1}`, {
        predicates: ['connected', 'shortcut'],
        maxHops: 20
      });
      return path;
    },
    () => Promise.resolve(),
    (result) => result === null || (Array.isArray(result) && result.length >= 0)
  ));

  // Dijkstra加权最短路径
  suite.addBenchmark(new Benchmark(
    'Dijkstra加权最短路径',
    '使用MinHeap优化的Dijkstra算法',
    () => Promise.resolve(),
    async () => {
      const path = db.shortestPathWeighted('n_0', `n_${nodeCount - 1}`, {
        predicate: 'connected',
        weightProperty: 'weight'
      });
      return path;
    },
    () => Promise.resolve(),
    (result) => result === null || (Array.isArray(result) && result.length >= 0)
  ));

  return suite;
}

/**
 * 创建聚合性能测试套件
 */
function createAggregationSuite() {
  const suite = new BenchmarkSuite(
    '聚合性能测试',
    '测试聚合管道和流式聚合性能'
  );

  let db, tempDir;
  const dataSize = CONFIG.MEDIUM_DATASET;

  suite.setGlobalSetup(async () => {
    tempDir = await mkdtemp(join(tmpdir(), 'synapsedb-bench-'));
    db = await NervusDB.open(join(tempDir, 'aggregation.synapsedb'));

    // 生成用户评分数据
    console.log('  🔧 生成聚合测试数据...');
    for (let userId = 0; userId < 1000; userId++) {
      if (userId % 100 === 0) process.stdout.write('.');

      for (let rating = 0; rating < dataSize / 1000; rating++) {
        const itemId = Math.floor(Math.random() * 500);
        const score = 1 + Math.floor(Math.random() * 5);
        const timestamp = Date.now() - Math.random() * 365 * 24 * 3600 * 1000;

        db.addFact(
          { subject: `user_${userId}`, predicate: 'rated', object: `item_${itemId}` },
          {
            subjectProperties: {
              type: 'user',
              region: `region_${userId % 10}`,
              age_group: ['young', 'adult', 'senior'][userId % 3]
            },
            edgeProperties: { score, timestamp }
          }
        );
      }
    }
    await db.flush();
    console.log('\n');
  });

  suite.setGlobalTeardown(async () => {
    if (db) await db.close();
    if (tempDir) await rm(tempDir, { recursive: true, force: true });
  });

  // 基础COUNT聚合
  suite.addBenchmark(new Benchmark(
    '基础计数聚合',
    '按谓语分组统计数量',
    () => Promise.resolve(),
    async () => {
      const results = db.aggregate()
        .match({ predicate: 'rated' })
        .groupBy(['predicate'])
        .count('total_ratings')
        .execute();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length > 0
  ));

  // 多维度分组聚合
  suite.addBenchmark(new Benchmark(
    '多维度分组聚合',
    '按用户地区分组计算平均评分',
    () => Promise.resolve(),
    async () => {
      const results = db.aggregate()
        .match({ predicate: 'rated' })
        .groupBy(['subjectProperties.region'])
        .avg('edgeProperties.score', 'avg_score')
        .count('rating_count')
        .orderBy('avg_score', 'DESC')
        .limit(5)
        .execute();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length <= 5
  ));

  // 流式聚合（内存高效）
  suite.addBenchmark(new Benchmark(
    '流式聚合执行',
    '使用流式处理大数据集聚合，避免内存溢出',
    () => Promise.resolve(),
    async () => {
      const results = await db.aggregate()
        .matchStream({ predicate: 'rated' }, { batchSize: 1000 })
        .groupBy(['subject'])
        .sum('edgeProperties.score', 'total_score')
        .avg('edgeProperties.score', 'avg_score')
        .count('rating_count')
        .orderBy('avg_score', 'DESC')
        .limit(10)
        .executeStreaming();
      return results;
    },
    () => Promise.resolve(),
    (result) => Array.isArray(result) && result.length <= 10
  ));

  return suite;
}

/**
 * 主函数：运行所有基准测试套件
 */
async function main() {
  console.log('🎯 NervusDB v1.1 综合性能基准测试');
  console.log('====================================');
  console.log(`Node.js: ${process.version}`);
  console.log(`平台: ${process.platform} ${process.arch}`);
  console.log(`CPU核心: ${require('os').cpus().length}`);
  console.log(`可用内存: ${Math.round(require('os').totalmem() / 1024 / 1024 / 1024)}GB\n`);

  const runner = new BenchmarkRunner({
    warmupRuns: 2,
    measurementRuns: 3,
    verbose: process.argv.includes('--verbose'),
    collectGC: typeof global.gc === 'function'
  });

  const allResults = [];
  const suites = [
    createInsertionSuite(),
    createQuerySuite(),
    createPathSuite(),
    createAggregationSuite()
  ];

  for (const suite of suites) {
    try {
      const result = await runner.run(suite);
      allResults.push(result);
    } catch (error) {
      console.error(`❌ 套件运行失败: ${error.message}`);
    }
  }

  // 生成综合报告
  console.log('\n🎉 所有基准测试完成！');

  const successfulTests = allResults.reduce((sum, suite) => sum + suite.summary.successful, 0);
  const failedTests = allResults.reduce((sum, suite) => sum + suite.summary.failed, 0);
  const totalTime = allResults.reduce((sum, suite) => sum + suite.summary.totalTime, 0);

  console.log('\n📊 总体摘要');
  console.log(`总测试数: ${successfulTests + failedTests}`);
  console.log(`成功: ${successfulTests}`);
  console.log(`失败: ${failedTests}`);
  console.log(`总耗时: ${(totalTime / 1000).toFixed(1)}秒`);

  // 生成JSON报告
  const reportPath = join(process.cwd(), 'benchmark-report.json');
  const report = {
    summary: { successfulTests, failedTests, totalTime },
    suites: allResults,
    generatedAt: new Date().toISOString()
  };

  runner.generateJsonReport({ results: allResults, summary: report.summary }, reportPath);

  process.exit(failedTests > 0 ? 1 : 0);
}

// 错误处理
process.on('uncaughtException', (error) => {
  console.error('❌ 未捕获的异常:', error);
  process.exit(1);
});

process.on('unhandledRejection', (reason, promise) => {
  console.error('❌ 未处理的Promise拒绝:', reason);
  process.exit(1);
});

main().catch(console.error);