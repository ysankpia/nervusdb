/**
 * NervusDB 性能基准测试框架入口
 *
 * 提供统一的基准测试接口和工具，支持多种测试场景和报告格式
 *
 * 注意：图算法、全文检索、空间索引的基准测试已归档到 _archive/ts-benchmark/
 */

// 核心类型和接口
export type {
  BenchmarkResult,
  BenchmarkTest,
  BenchmarkSuite,
  BenchmarkConfig,
  BenchmarkReport,
  BenchmarkRunner,
  BenchmarkReporter,
  PerformanceMonitor,
  PerformanceMetrics,
  DataGenerationConfig,
  EnvironmentInfo,
  BenchmarkSummary,
  RegressionConfig,
  RegressionResult,
  LoadTestConfig,
  LoadTestResult,
  MemoryLeakConfig,
  MemoryLeakResult,
  CPUProfilingConfig,
  CPUProfilingResult,
} from './types.js';

// 核心实现
export {
  BenchmarkRunnerImpl,
  PerformanceMonitorImpl,
  BenchmarkUtils,
  benchmark,
  memoryMonitor,
  performanceThreshold,
} from './runner.js';

// 预定义测试套件（仅保留核心）
export { synapseDBCoreSuite, allBenchmarkSuites } from './suites.js';

// 报告生成器
export { BenchmarkReporterImpl, ReportFormatter } from './reporter.js';

// 导入实现与类型
import { BenchmarkRunnerImpl, BenchmarkUtils } from './runner.js';
import {
  BenchmarkReporterImpl,
  BenchmarkReporterImpl as BenchmarkReporterImplType,
} from './reporter.js';
import { allBenchmarkSuites } from './suites.js';
import type {
  BenchmarkSuite,
  BenchmarkResult,
  BenchmarkReport,
  RegressionConfig,
  RegressionResult,
} from './types.js';

/**
 * 基准测试管理器 - 提供简化的API接口
 */
export class BenchmarkManager {
  private runner: BenchmarkRunnerImpl;
  private reporter: BenchmarkReporterImplType;

  constructor() {
    this.runner = new BenchmarkRunnerImpl();
    this.reporter = new BenchmarkReporterImpl();
  }

  async runSuite(suite: BenchmarkSuite): Promise<BenchmarkResult[]> {
    return this.runner.runSuite(suite);
  }

  async runAllSuites(): Promise<BenchmarkReport> {
    const { allBenchmarkSuites } = await import('./suites.js');
    return this.runner.runAll(allBenchmarkSuites);
  }

  async runSuites(suites: BenchmarkSuite[]): Promise<BenchmarkReport> {
    return this.runner.runAll(suites);
  }

  generateConsoleReport(report: BenchmarkReport): string {
    return this.reporter.generateConsoleReport(report);
  }

  generateHTMLReport(report: BenchmarkReport): string {
    return this.reporter.generateHTMLReport(report);
  }

  generateJSONReport(report: BenchmarkReport): string {
    return this.reporter.generateJSONReport(report);
  }

  generateCSVReport(report: BenchmarkReport): string {
    return this.reporter.generateCSVReport(report);
  }

  async benchmark(
    options: {
      suites?: BenchmarkSuite[];
      outputFormats?: ('console' | 'html' | 'json' | 'csv')[];
      outputDir?: string;
    } = {},
  ): Promise<{
    report: BenchmarkReport;
    outputs: { format: string; content: string; path?: string }[];
  }> {
    const { suites = allBenchmarkSuites, outputFormats = ['console'], outputDir } = options;

    console.log('🏁 开始运行 NervusDB 性能基准测试...\n');

    const report = await this.runSuites(suites);
    const outputs: { format: string; content: string; path?: string }[] = [];

    for (const format of outputFormats) {
      let content: string;
      let fileName: string;

      switch (format) {
        case 'console':
          content = this.generateConsoleReport(report);
          console.log(content);
          outputs.push({ format, content });
          break;

        case 'html':
          content = this.generateHTMLReport(report);
          fileName = `benchmark-report-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.html`;
          outputs.push({
            format,
            content,
            path: outputDir ? `${outputDir}/${fileName}` : fileName,
          });
          break;

        case 'json':
          content = this.generateJSONReport(report);
          fileName = `benchmark-report-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.json`;
          outputs.push({
            format,
            content,
            path: outputDir ? `${outputDir}/${fileName}` : fileName,
          });
          break;

        case 'csv':
          content = this.generateCSVReport(report);
          fileName = `benchmark-report-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.csv`;
          outputs.push({
            format,
            content,
            path: outputDir ? `${outputDir}/${fileName}` : fileName,
          });
          break;
      }
    }

    return { report, outputs };
  }

  async runRegressionTest(
    baselineReport: BenchmarkReport,
    config?: RegressionConfig,
  ): Promise<RegressionResult[]> {
    const currentReport = await this.runAllSuites();
    const regressions: RegressionResult[] = [];

    const threshold = config?.regressionThreshold || 10;
    const metricsToCheck = config?.metricsToCheck || [
      'executionTime',
      'memoryUsage',
      'operationsPerSecond',
    ];

    for (const currentResult of currentReport.results) {
      const baselineResult = baselineReport.results.find((r) => r.name === currentResult.name);
      if (!baselineResult) continue;

      for (const metric of metricsToCheck) {
        if (!(metric in currentResult) || !(metric in baselineResult)) continue;

        const currentValue = currentResult[metric] as number;
        const baselineValue = baselineResult[metric] as number;

        if (baselineValue === 0) continue;

        const changePercent = BenchmarkUtils.calculateChangePercent(currentValue, baselineValue);
        let isRegression = false;

        switch (metric) {
          case 'executionTime':
          case 'memoryUsage':
          case 'averageLatency':
          case 'minLatency':
          case 'maxLatency':
          case 'p95Latency':
          case 'p99Latency':
            isRegression = changePercent > threshold;
            break;
          case 'operationsPerSecond':
            isRegression = changePercent < -threshold;
            break;
        }

        regressions.push({
          testName: currentResult.name,
          passed: !isRegression,
          currentValue,
          baselineValue,
          changePercent,
          metric: metric as string,
          details: isRegression ? `性能退化超过阈值 ${threshold}%` : undefined,
        });
      }
    }

    return regressions;
  }
}

export function createBenchmarkManager(): BenchmarkManager {
  return new BenchmarkManager();
}

export async function runBenchmark(options?: {
  suites?: BenchmarkSuite[];
  outputFormats?: ('console' | 'html' | 'json' | 'csv')[];
  outputDir?: string;
}): Promise<BenchmarkReport> {
  const manager = createBenchmarkManager();
  const result = await manager.benchmark(options);
  return result.report;
}

export async function runCoreBenchmark(): Promise<BenchmarkReport> {
  const { synapseDBCoreSuite } = await import('./suites.js');
  const manager = createBenchmarkManager();
  return manager.runSuites([synapseDBCoreSuite]);
}

// 已移除的基准测试函数（功能已归档）
// - runFullTextBenchmark -> _archive/ts-benchmark/
// - runGraphAlgorithmsBenchmark -> _archive/ts-benchmark/
// - runSpatialBenchmark -> _archive/ts-benchmark/

export default BenchmarkManager;
