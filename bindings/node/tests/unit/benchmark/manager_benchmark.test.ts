import { describe, it, expect } from 'vitest';
import { BenchmarkManager } from '@/benchmark/index.ts';
import type { BenchmarkSuite, BenchmarkConfig, BenchmarkResult } from '@/benchmark/types.ts';

describe('BenchmarkManager · benchmark() 最小路径', () => {
  it('benchmark: 自定义套件 + 多格式输出', async () => {
    const mgr = new BenchmarkManager();

    const tinyTest = {
      name: 'noop2',
      description: 'fast noop2',
      test: async (_config: BenchmarkConfig): Promise<BenchmarkResult> => ({
        name: 'noop2',
        description: 'fast noop2',
        executionTime: 1,
        memoryUsage: 0,
        operations: 1,
        operationsPerSecond: 1000,
        averageLatency: 1,
        minLatency: 1,
        maxLatency: 1,
        p95Latency: 1,
        p99Latency: 1,
        dataSize: 0,
        timestamp: new Date(),
      }),
      config: { warmupRuns: 0, runs: 1, timeout: 1000 },
    } satisfies BenchmarkSuite['benchmarks'][number];

    const suite: BenchmarkSuite = {
      name: 'mini-2',
      description: 'tiny suite 2',
      benchmarks: [tinyTest],
      config: { warmupRuns: 0, runs: 1 },
    };

    const { outputs } = await mgr.benchmark({
      suites: [suite],
      outputFormats: ['console', 'json'],
    });
    expect(outputs.length).toBe(2);
    expect(outputs[0].format).toBe('console');
    expect(outputs[1].format).toBe('json');
  });
});
