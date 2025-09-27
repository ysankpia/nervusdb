/**
 * SynapseDB 基准测试套件
 *
 * 为各个模块提供完整的性能测试用例
 */

import { SynapseDB } from '../synapseDb.js';
import { MemoryGraph } from '../algorithms/graph.js';
import { GraphAlgorithmSuiteImpl } from '../algorithms/suite.js';
import { FullTextSearchFactory } from '../fulltext/engine.js';
import { SpatialGeometryImpl } from '../spatial/geometry.js';
import { BenchmarkSuite } from './types.js';
import { BenchmarkUtils } from './runner.js';

/**
 * 数据生成器
 */
class DataGenerator {
  /**
   * 生成三元组事实数据
   */
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

  /**
   * 生成文档数据
   */
  static generateDocuments(count: number): Array<{ id: string; content: string; title: string }> {
    const documents = [];
    const titles = ['技术文档', '产品介绍', '用户手册', '开发指南', '最佳实践'];
    const contents = [
      '这是一份详细的技术文档，包含了系统的架构设计和实现细节。',
      '产品具有强大的功能和优秀的性能表现，适合各种应用场景。',
      '用户可以通过简单的操作快速上手，享受便捷的使用体验。',
      '开发团队遵循最佳实践，确保代码质量和系统稳定性。',
      '系统支持多种配置选项，可以根据需求进行灵活调整。',
    ];

    for (let i = 0; i < count; i++) {
      documents.push({
        id: `doc_${i}`,
        title: `${titles[i % titles.length]} ${Math.floor(i / titles.length)}`,
        content: contents[Math.floor(Math.random() * contents.length)] + ` ID: ${i}`,
      });
    }

    return documents;
  }

  /**
   * 生成图数据
   */
  static generateGraph(
    nodeCount: number,
    edgeCount: number,
  ): { nodes: string[]; edges: Array<{ source: string; target: string; weight?: number }> } {
    const nodes = [];
    for (let i = 0; i < nodeCount; i++) {
      nodes.push(`node_${i}`);
    }

    const edges = [];
    for (let i = 0; i < edgeCount; i++) {
      const source = nodes[Math.floor(Math.random() * nodes.length)];
      const target = nodes[Math.floor(Math.random() * nodes.length)];
      if (source !== target) {
        edges.push({
          source,
          target,
          weight: Math.random() * 10,
        });
      }
    }

    return { nodes, edges };
  }

  /**
   * 生成空间坐标数据
   */
  static generateCoordinates(count: number): Array<[number, number]> {
    const coordinates: Array<[number, number]> = [];
    for (let i = 0; i < count; i++) {
      coordinates.push([
        BenchmarkUtils.generateRandomInt(-180, 180), // 经度
        BenchmarkUtils.generateRandomInt(-90, 90), // 纬度
      ]);
    }
    return coordinates;
  }

  /**
   * 播种随机数生成器
   */
  private static seededRandom(seed: number) {
    return function () {
      const x = Math.sin(seed++) * 10000;
      return x - Math.floor(x);
    };
  }
}

/**
 * SynapseDB 核心功能基准测试套件
 */
export const synapseDBCoreSuite: BenchmarkSuite = {
  name: 'SynapseDB Core',
  description: 'SynapseDB 核心功能性能测试',
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
        const db = await SynapseDB.open(':memory:');
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
        const db = await SynapseDB.open(':memory:');
        const facts = DataGenerator.generateFacts(dataSize);

        // 预填充数据
        for (const fact of facts) {
          db.addFact(fact);
        }
        await db.flush();

        // 测试查询性能
        const queryCount = Math.min(1000, dataSize);
        const start = performance.now();

        for (let i = 0; i < queryCount; i++) {
          const randomSubject = facts[Math.floor(Math.random() * facts.length)].subject;
          db.find({ subject: randomSubject }).all();
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
      name: '链式查询',
      description: '测试链式联想查询性能',
      test: async (config) => {
        const dataSize = config.dataGeneration?.size || 1000;
        const db = await SynapseDB.open(':memory:');

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
          db.find({ subject: 'node_0' })
            .follow('connects_to')
            .follow('connects_to')
            .follow('connects_to')
            .all();
        }

        const executionTime = performance.now() - start;
        await db.close();

        return {
          name: '链式查询',
          description: '测试链式联想查询性能',
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
 * 全文搜索基准测试套件
 */
export const fullTextSearchSuite: BenchmarkSuite = {
  name: 'Full-Text Search',
  description: '全文搜索引擎性能测试',
  config: {
    warmupRuns: 2,
    runs: 3,
    timeout: 45000,
  },
  benchmarks: [
    {
      name: '文档索引',
      description: '测试文档索引创建性能',
      test: async (config) => {
        const dataSize = config.dataGeneration?.size || 1000;
        const engine = FullTextSearchFactory.createEngine();
        const documents = DataGenerator.generateDocuments(dataSize);

        await engine.createIndex(
          'test',
          FullTextSearchFactory.createDefaultConfig(['title', 'content']),
        );

        const start = performance.now();
        for (const doc of documents) {
          await engine.indexDocument('test', {
            id: doc.id,
            fields: new Map([
              ['title', doc.title],
              ['content', doc.content],
            ]),
            tokens: [],
            timestamp: new Date(),
          });
        }
        const executionTime = performance.now() - start;

        return {
          name: '文档索引',
          description: '测试文档索引创建性能',
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
      config: { dataGeneration: { size: 5000, type: 'documents' } },
    },
    {
      name: '全文搜索',
      description: '测试全文搜索查询性能',
      test: async (config) => {
        const dataSize = config.dataGeneration?.size || 1000;
        const engine = FullTextSearchFactory.createEngine();
        const documents = DataGenerator.generateDocuments(dataSize);

        await engine.createIndex(
          'test',
          FullTextSearchFactory.createDefaultConfig(['title', 'content']),
        );

        // 预建索引
        for (const doc of documents) {
          await engine.indexDocument('test', {
            id: doc.id,
            fields: new Map([
              ['title', doc.title],
              ['content', doc.content],
            ]),
            tokens: [],
            timestamp: new Date(),
          });
        }

        // 测试搜索性能
        const queries = ['技术', '产品', '用户', '开发', '系统'];
        const queryCount = 500;
        const start = performance.now();

        for (let i = 0; i < queryCount; i++) {
          const query = queries[i % queries.length];
          await engine.search('test', query, { maxResults: 50 });
        }

        const executionTime = performance.now() - start;

        return {
          name: '全文搜索',
          description: '测试全文搜索查询性能',
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
      config: { dataGeneration: { size: 5000, type: 'documents' } },
    },
  ],
};

/**
 * 图算法基准测试套件
 */
export const graphAlgorithmsSuite: BenchmarkSuite = {
  name: 'Graph Algorithms',
  description: '图算法库性能测试',
  config: {
    warmupRuns: 2,
    runs: 3,
    timeout: 60000,
  },
  benchmarks: [
    {
      name: 'PageRank计算',
      description: '测试PageRank算法性能',
      test: (config) => {
        const nodeCount = Number(
          (config.dataGeneration?.params as { nodeCount?: number })?.nodeCount ?? 1000,
        );
        const edgeCount = Number(
          (config.dataGeneration?.params as { edgeCount?: number })?.edgeCount ?? 3000,
        );

        const graph = new MemoryGraph();
        const { nodes, edges } = DataGenerator.generateGraph(nodeCount, edgeCount);

        // 构建图
        for (const node of nodes) {
          graph.addNode({ id: node, value: node });
        }
        for (const edge of edges) {
          graph.addEdge({
            source: edge.source,
            target: edge.target,
            type: 'link',
            weight: edge.weight,
          });
        }

        const suite = new GraphAlgorithmSuiteImpl(graph);

        const start = performance.now();
        const result = suite.centrality.pageRank({ maxIterations: 100 });
        const executionTime = performance.now() - start;

        return {
          name: 'PageRank计算',
          description: '测试PageRank算法性能',
          executionTime,
          memoryUsage: 0,
          operations: nodeCount,
          operationsPerSecond: (nodeCount / executionTime) * 1000,
          averageLatency: executionTime / nodeCount,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize: nodeCount,
          timestamp: new Date(),
          metrics: {
            convergence: result.stats.standardDeviation,
            topScore: result.stats.max,
          },
        };
      },
      config: {
        dataGeneration: {
          size: 1000,
          type: 'nodes',
          params: { nodeCount: 1000, edgeCount: 3000 },
        },
      },
    },
    {
      name: 'Dijkstra路径查找',
      description: '测试Dijkstra最短路径算法性能',
      test: (config) => {
        const nodeCount = Number(
          (config.dataGeneration?.params as { nodeCount?: number })?.nodeCount ?? 500,
        );
        const edgeCount = Number(
          (config.dataGeneration?.params as { edgeCount?: number })?.edgeCount ?? 1500,
        );

        const graph = new MemoryGraph();
        const { nodes, edges } = DataGenerator.generateGraph(nodeCount, edgeCount);

        // 构建图
        for (const node of nodes) {
          graph.addNode({ id: node, value: node });
        }
        for (const edge of edges) {
          graph.addEdge({
            source: edge.source,
            target: edge.target,
            type: 'path',
            weight: edge.weight,
          });
        }

        const suite = new GraphAlgorithmSuiteImpl(graph);

        // 测试多次路径查找
        const pathCount = 100;
        const start = performance.now();

        for (let i = 0; i < pathCount; i++) {
          const sourceIndex = Math.floor(Math.random() * nodes.length);
          const source = nodes[sourceIndex];
          suite.path.dijkstra(source);
        }

        const executionTime = performance.now() - start;

        return {
          name: 'Dijkstra路径查找',
          description: '测试Dijkstra最短路径算法性能',
          executionTime,
          memoryUsage: 0,
          operations: pathCount,
          operationsPerSecond: (pathCount / executionTime) * 1000,
          averageLatency: executionTime / pathCount,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize: nodeCount,
          timestamp: new Date(),
        };
      },
      config: {
        dataGeneration: {
          size: 500,
          type: 'nodes',
          params: { nodeCount: 500, edgeCount: 1500 },
        },
      },
    },
    {
      name: '社区发现',
      description: '测试Louvain社区发现算法性能',
      test: (config) => {
        const nodeCount = Number(
          (config.dataGeneration?.params as { nodeCount?: number })?.nodeCount ?? 800,
        );
        const edgeCount = Number(
          (config.dataGeneration?.params as { edgeCount?: number })?.edgeCount ?? 2400,
        );

        const graph = new MemoryGraph();
        const { nodes, edges } = DataGenerator.generateGraph(nodeCount, edgeCount);

        // 构建图
        for (const node of nodes) {
          graph.addNode({ id: node, value: node });
        }
        for (const edge of edges) {
          graph.addEdge({
            source: edge.source,
            target: edge.target,
            type: 'connection',
            weight: edge.weight,
          });
        }

        const suite = new GraphAlgorithmSuiteImpl(graph);

        const start = performance.now();
        const result = suite.community.louvain({ maxIterations: 50 });
        const executionTime = performance.now() - start;

        return {
          name: '社区发现',
          description: '测试Louvain社区发现算法性能',
          executionTime,
          memoryUsage: 0,
          operations: nodeCount,
          operationsPerSecond: (nodeCount / executionTime) * 1000,
          averageLatency: executionTime / nodeCount,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize: nodeCount,
          timestamp: new Date(),
          metrics: {
            communities: result.communityCount,
            modularity: result.modularity,
          },
        };
      },
      config: {
        dataGeneration: {
          size: 800,
          type: 'nodes',
          params: { nodeCount: 800, edgeCount: 2400 },
        },
      },
    },
  ],
};

/**
 * 空间几何计算基准测试套件
 */
export const spatialGeometrySuite: BenchmarkSuite = {
  name: 'Spatial Geometry',
  description: '空间几何计算性能测试',
  config: {
    warmupRuns: 2,
    runs: 3,
    timeout: 30000,
  },
  benchmarks: [
    {
      name: '距离计算',
      description: '测试两点间距离计算性能',
      test: (config) => {
        const dataSize = config.dataGeneration?.size || 10000;
        const coordinates = DataGenerator.generateCoordinates(dataSize);
        const spatial = new SpatialGeometryImpl();

        const operationCount = 5000;
        const start = performance.now();

        for (let i = 0; i < operationCount; i++) {
          const idx1 = Math.floor(Math.random() * coordinates.length);
          const idx2 = Math.floor(Math.random() * coordinates.length);

          const point1 = { type: 'Point' as const, coordinates: coordinates[idx1] };
          const point2 = { type: 'Point' as const, coordinates: coordinates[idx2] };

          spatial.distance(point1, point2);
        }

        const executionTime = performance.now() - start;

        return {
          name: '距离计算',
          description: '测试两点间距离计算性能',
          executionTime,
          memoryUsage: 0,
          operations: operationCount,
          operationsPerSecond: (operationCount / executionTime) * 1000,
          averageLatency: executionTime / operationCount,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize,
          timestamp: new Date(),
        };
      },
      config: { dataGeneration: { size: 10000, type: 'coordinates' } },
    },
    {
      name: '边界框计算',
      description: '测试几何对象边界框计算性能',
      test: (config) => {
        const dataSize = config.dataGeneration?.size || 5000;
        const coordinates = DataGenerator.generateCoordinates(dataSize);
        const spatial = new SpatialGeometryImpl();

        // 创建多边形
        const polygons = [];
        for (let i = 0; i < dataSize / 10; i++) {
          const ringSize = 5 + Math.floor(Math.random() * 10);
          const ring = [];
          for (let j = 0; j < ringSize; j++) {
            ring.push(coordinates[Math.floor(Math.random() * coordinates.length)]);
          }
          ring.push(ring[0]); // 闭合环
          polygons.push({
            type: 'Polygon' as const,
            coordinates: [ring],
          });
        }

        const start = performance.now();
        for (const polygon of polygons) {
          spatial.bounds(polygon);
        }
        const executionTime = performance.now() - start;

        return {
          name: '边界框计算',
          description: '测试几何对象边界框计算性能',
          executionTime,
          memoryUsage: 0,
          operations: polygons.length,
          operationsPerSecond: (polygons.length / executionTime) * 1000,
          averageLatency: executionTime / polygons.length,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize,
          timestamp: new Date(),
        };
      },
      config: { dataGeneration: { size: 5000, type: 'coordinates' } },
    },
  ],
};

/**
 * 所有基准测试套件
 */
export const allBenchmarkSuites: BenchmarkSuite[] = [
  synapseDBCoreSuite,
  fullTextSearchSuite,
  graphAlgorithmsSuite,
  spatialGeometrySuite,
];
