/**
 * SynapseDB 性能基准测试框架入口
 *
 * 提供统一的基准测试接口和工具，支持多种测试场景和报告格式
 *
 * @deprecated 此内部基准测试框架将在 v2.0 移除
 * @see benchmarks/*.mjs - 推荐直接使用外部基准测试脚本
 * @see benchmarks/run-all.mjs - 统一入口脚本
 *
 * 迁移路径：
 * - 命令行用户：继续使用 `pnpm benchmark` CLI（内部已迁移到外部脚本）
 * - 高级用户：直接运行 `node benchmarks/run-all.mjs` 获得最佳体验
 * - 程序化使用：不推荐（此模块未作为公开 API）
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

// 预定义测试套件
export {
  synapseDBCoreSuite,
  fullTextSearchSuite,
  graphAlgorithmsSuite,
  spatialGeometrySuite,
  allBenchmarkSuites,
} from './suites.js';

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

  /**
   * 运行单个测试套件
   */
  async runSuite(suite: BenchmarkSuite): Promise<BenchmarkResult[]> {
    return this.runner.runSuite(suite);
  }

  /**
   * 运行所有预定义测试套件
   */
  async runAllSuites(): Promise<BenchmarkReport> {
    const { allBenchmarkSuites } = await import('./suites.js');
    return this.runner.runAll(allBenchmarkSuites);
  }

  /**
   * 运行指定的测试套件集合
   */
  async runSuites(suites: BenchmarkSuite[]): Promise<BenchmarkReport> {
    return this.runner.runAll(suites);
  }

  /**
   * 生成控制台报告
   */
  generateConsoleReport(report: BenchmarkReport): string {
    return this.reporter.generateConsoleReport(report);
  }

  /**
   * 生成HTML报告
   */
  generateHTMLReport(report: BenchmarkReport): string {
    return this.reporter.generateHTMLReport(report);
  }

  /**
   * 生成JSON报告
   */
  generateJSONReport(report: BenchmarkReport): string {
    return this.reporter.generateJSONReport(report);
  }

  /**
   * 生成CSV报告
   */
  generateCSVReport(report: BenchmarkReport): string {
    return this.reporter.generateCSVReport(report);
  }

  /**
   * 运行完整基准测试并生成报告
   */
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

    console.log('🏁 开始运行 SynapseDB 性能基准测试...\n');

    // 运行测试
    const report = await this.runSuites(suites);

    // 生成报告
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

  /**
   * 运行性能回归检测
   */
  async runRegressionTest(
    baselineReport: BenchmarkReport,
    config?: RegressionConfig,
  ): Promise<RegressionResult[]> {
    const currentReport = await this.runAllSuites();
    const regressions: RegressionResult[] = [];

    const threshold = config?.regressionThreshold || 10; // 默认10%性能退化阈值
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

        // 根据指标类型判断是否为回归
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

/**
 * 创建基准测试管理器实例
 */
export function createBenchmarkManager(): BenchmarkManager {
  return new BenchmarkManager();
}

/**
 * 快捷函数：运行完整基准测试
 */
export async function runBenchmark(options?: {
  suites?: BenchmarkSuite[];
  outputFormats?: ('console' | 'html' | 'json' | 'csv')[];
  outputDir?: string;
}): Promise<BenchmarkReport> {
  const manager = createBenchmarkManager();
  const result = await manager.benchmark(options);
  return result.report;
}

/**
 * 快捷函数：运行核心功能基准测试
 */
export async function runCoreBenchmark(): Promise<BenchmarkReport> {
  const { synapseDBCoreSuite } = await import('./suites.js');
  const manager = createBenchmarkManager();
  return manager.runSuites([synapseDBCoreSuite]);
}

/**
 * 快捷函数：运行全文搜索基准测试
 */
export async function runFullTextBenchmark(): Promise<BenchmarkReport> {
  const { fullTextSearchSuite } = await import('./suites.js');
  const manager = createBenchmarkManager();
  return manager.runSuites([fullTextSearchSuite]);
}

/**
 * 快捷函数：运行图算法基准测试
 */
export async function runGraphAlgorithmsBenchmark(): Promise<BenchmarkReport> {
  const { graphAlgorithmsSuite } = await import('./suites.js');
  const manager = createBenchmarkManager();
  return manager.runSuites([graphAlgorithmsSuite]);
}

/**
 * 快捷函数：运行空间几何基准测试
 */
export async function runSpatialBenchmark(): Promise<BenchmarkReport> {
  const { spatialGeometrySuite } = await import('./suites.js');
  const manager = createBenchmarkManager();
  return manager.runSuites([spatialGeometrySuite]);
}

// 默认导出
export default BenchmarkManager;
