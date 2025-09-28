import { describe, it, expect } from 'vitest';
import { BenchmarkRunnerImpl, BenchmarkUtils } from '@/benchmark/runner.ts';
import { BenchmarkReporterImpl } from '@/benchmark/reporter.ts';

describe('基准测试 · 运行器与报告', async () => {
  it('runSuite/runs=1 生成报告并格式化', async () => {
    const runner = new BenchmarkRunnerImpl();
    const suite = {
      name: 'mini',
      description: 'mini suite',
      config: { runs: 1, warmupRuns: 0, collectLatencyStats: true },
      benchmarks: [
        {
          name: 'noop',
          description: 'do nothing',
          test: async () => {
            const t0 = performance.now();
            // no-op
            return {
              name: 'noop',
              description: 'do nothing',
              executionTime: performance.now() - t0,
              memoryUsage: 0,
              operations: 1,
              operationsPerSecond: 1,
              averageLatency: 0,
              minLatency: 0,
              maxLatency: 0,
              p95Latency: 0,
              p99Latency: 0,
              dataSize: 0,
              timestamp: new Date(),
            };
          },
        },
      ],
    } as const;

    const report = await runner.runAll([suite as any]);
    expect(report.results.length).toBe(1);
    const reporter = new BenchmarkReporterImpl();
    expect(reporter.generateConsoleReport(report)).toContain('性能基准测试报告');
    expect(reporter.generateJSONReport(report)).toContain('results');
    expect(reporter.generateCSVReport(report)).toContain('测试名称');
    const html = reporter.generateHTMLReport(report);
    expect(html.includes('<!DOCTYPE html>') || html.includes('<html')).toBe(true);

    // utils 小函数
    expect(BenchmarkUtils.formatBytes(1024)).toContain('KB');
    expect(BenchmarkUtils.formatTime(12.34)).toContain('ms');
  });
});
