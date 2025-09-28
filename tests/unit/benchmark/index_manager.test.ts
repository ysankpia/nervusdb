import { describe, it, expect } from 'vitest';
import { BenchmarkManager } from '@/benchmark/index.ts';
import type { BenchmarkSuite, BenchmarkConfig, BenchmarkResult } from '@/benchmark/types.ts';

describe('BenchmarkManager · 轻量套件与报告生成', () => {
  it('runSuites + generate console/html/json/csv', async () => {
    const mgr = new BenchmarkManager();

    const tinyTest = {
      name: 'noop',
      description: 'fast noop',
      test: async (config: BenchmarkConfig): Promise<BenchmarkResult> => {
        // 使用最小 runs、无预热
        void config;
        const executionTime = 1;
        return {
          name: 'noop',
          description: 'fast noop',
          executionTime,
          memoryUsage: 0,
          operations: 1,
          operationsPerSecond: 1000,
          averageLatency: executionTime,
          minLatency: executionTime,
          maxLatency: executionTime,
          p95Latency: executionTime,
          p99Latency: executionTime,
          dataSize: 0,
          timestamp: new Date(),
        };
      },
      config: { warmupRuns: 0, runs: 1, timeout: 1000 },
    } satisfies BenchmarkSuite['benchmarks'][number];

    const suite: BenchmarkSuite = {
      name: 'mini',
      description: 'tiny suite',
      benchmarks: [tinyTest],
      config: { warmupRuns: 0, runs: 1, timeout: 1000 },
    };

    const report = await mgr.runSuites([suite]);
    expect(Array.isArray(report.results)).toBe(true);
    expect(report.results[0].name).toBe('noop');

    const consoleTxt = mgr.generateConsoleReport(report);
    expect(consoleTxt.includes('SynapseDB 性能基准测试报告')).toBe(true);
    expect(consoleTxt.includes('noop')).toBe(true);
    const html = mgr.generateHTMLReport(report);
    expect(html.includes('<html')).toBe(true);
    const json = mgr.generateJSONReport(report);
    expect(json.includes('"results"')).toBe(true);
    const csv = mgr.generateCSVReport(report);
    expect(csv.split('\n').length).toBeGreaterThan(1);
  });
});
