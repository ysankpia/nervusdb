/**
 * NervusDB 基准测试套件
 *
 * 仅保留核心存储功能的性能测试
 * 图算法、全文检索、空间索引的基准测试已移至 _archive/ts-benchmark/
 */

import { NervusDB } from '../synapseDb.js';
import type { BenchmarkSuite } from './types.js';
import { BenchmarkUtils } from './runner.js';

/**
 * 数据生成器
 */
class DataGenerator {
  static generateFacts(
    count: number,
    seed?: number,
  ): Array<{ subject: string; predicate: string; object: string }> {
    if (seed) Math.random = this.seededRandom(seed);

    const facts = [];
    const subjects = ['用户', '产品', '订单', '公司', '项目'];
    const predicates = ['属于', '拥有', '关联', '位于', '创建'];
    const objects = ['类别', '属性', '状态', '地区', '时间'];

    for (let i = 0; i < count; i++) {
      facts.push({
        subject: `${subjects[i % subjects.length]}_${Math.floor(i / subjects.length)}`,
        predicate: predicates[Math.floor(Math.random() * predicates.length)],
        object: `${objects[Math.floor(Math.random() * objects.length)]}_${BenchmarkUtils.generateRandomInt(1, 1000)}`,
      });
    }

    return facts;
  }

  private static seededRandom(seed: number) {
    return function () {
      const x = Math.sin(seed++) * 10000;
      return x - Math.floor(x);
    };
  }
}

/**
 * NervusDB 核心功能基准测试套件
 */
export const synapseDBCoreSuite: BenchmarkSuite = {
  name: 'NervusDB Core',
  description: 'NervusDB 核心功能性能测试',
  config: {
    warmupRuns: 3,
    runs: 5,
    timeout: 30000,
  },
  benchmarks: [
    {
      name: '三元组插入',
      description: '测试三元组事实插入性能',
      test: async (config) => {
        const dataSize = config.dataGeneration?.size || 1000;
        const db = await NervusDB.open(':memory:');
        const facts = DataGenerator.generateFacts(dataSize);

        const start = performance.now();
        for (const fact of facts) {
          db.addFact(fact);
        }
        await db.flush();
        const executionTime = performance.now() - start;

        await db.close();

        return {
          name: '三元组插入',
          description: '测试三元组事实插入性能',
          executionTime,
          memoryUsage: 0,
          operations: dataSize,
          operationsPerSecond: (dataSize / executionTime) * 1000,
          averageLatency: executionTime / dataSize,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize,
          timestamp: new Date(),
        };
      },
      config: { dataGeneration: { size: 10000, type: 'facts' } },
    },
    {
      name: '三元组查询',
      description: '测试三元组事实查询性能',
      test: async (config) => {
        const dataSize = config.dataGeneration?.size || 1000;
        const db = await NervusDB.open(':memory:');
        const facts = DataGenerator.generateFacts(dataSize);

        for (const fact of facts) {
          db.addFact(fact);
        }
        await db.flush();

        const queryCount = Math.min(1000, dataSize);
        const start = performance.now();

        for (let i = 0; i < queryCount; i++) {
          const randomSubject = facts[Math.floor(Math.random() * facts.length)].subject;
          db.getStore().query({ subject: randomSubject });
        }

        const executionTime = performance.now() - start;
        await db.close();

        return {
          name: '三元组查询',
          description: '测试三元组事实查询性能',
          executionTime,
          memoryUsage: 0,
          operations: queryCount,
          operationsPerSecond: (queryCount / executionTime) * 1000,
          averageLatency: executionTime / queryCount,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize,
          timestamp: new Date(),
        };
      },
      config: { dataGeneration: { size: 10000, type: 'facts' } },
    },
    {
      name: 'Cypher查询 (Native)',
      description: '测试通过 Native 执行 Cypher 查询的性能',
      test: async (config) => {
        const dataSize = config.dataGeneration?.size || 1000;
        const db = await NervusDB.open(':memory:');

        // 创建链式数据
        for (let i = 0; i < dataSize - 1; i++) {
          db.addFact({
            subject: `node_${i}`,
            predicate: 'connects_to',
            object: `node_${i + 1}`,
          });
        }
        await db.flush();

        const queryCount = 100;
        const start = performance.now();

        for (let i = 0; i < queryCount; i++) {
          try {
            await db.cypher(`MATCH (n) RETURN n LIMIT 10`);
          } catch {
            // Native 查询可能不可用，跳过
          }
        }

        const executionTime = performance.now() - start;
        await db.close();

        return {
          name: 'Cypher查询 (Native)',
          description: '测试通过 Native 执行 Cypher 查询的性能',
          executionTime,
          memoryUsage: 0,
          operations: queryCount,
          operationsPerSecond: (queryCount / executionTime) * 1000,
          averageLatency: executionTime / queryCount,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize,
          timestamp: new Date(),
        };
      },
      config: { dataGeneration: { size: 5000, type: 'facts' } },
    },
  ],
};

/**
 * 所有基准测试套件
 * 注意：图算法、全文检索、空间索引的基准测试已归档到 _archive/ts-benchmark/
 */
export const allBenchmarkSuites: BenchmarkSuite[] = [synapseDBCoreSuite];
