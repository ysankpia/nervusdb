/**
 * 基准测试运行器实现
 *
 * 提供统一的性能测试执行框架，支持多种测试模式和结果收集
 */

import type {
  BenchmarkTest,
  BenchmarkSuite,
  BenchmarkResult,
  BenchmarkConfig,
  BenchmarkReport,
  BenchmarkRunner,
  PerformanceMonitor,
  PerformanceMetrics,
  EnvironmentInfo,
  BenchmarkSummary,
} from './types.js';
import os from 'os';

/**
 * 性能监控器实现
 */
export class PerformanceMonitorImpl implements PerformanceMonitor {
  private startTime: number = 0;
  private endTime: number = 0;
  private startMemory: number = 0;
  private endMemory: number = 0;
  private peakMemory: number = 0;
  private memoryInterval?: NodeJS.Timeout;

  start(): void {
    // 强制垃圾回收（如果可用）
    if (global.gc) {
      global.gc();
    }

    this.startTime = performance.now();
    this.startMemory = process.memoryUsage().heapUsed;
    this.peakMemory = this.startMemory;

    // 监控内存峰值
    this.memoryInterval = setInterval(() => {
      const currentMemory = process.memoryUsage().heapUsed;
      if (currentMemory > this.peakMemory) {
        this.peakMemory = currentMemory;
      }
    }, 10);
  }

  stop(): PerformanceMetrics {
    this.endTime = performance.now();
    this.endMemory = process.memoryUsage().heapUsed;

    if (this.memoryInterval) {
      clearInterval(this.memoryInterval);
      this.memoryInterval = undefined;
    }

    return {
      startTime: this.startTime,
      endTime: this.endTime,
      executionTime: this.endTime - this.startTime,
      startMemory: this.startMemory,
      endMemory: this.endMemory,
      memoryDelta: this.endMemory - this.startMemory,
      peakMemory: this.peakMemory,
    };
  }

  reset(): void {
    this.startTime = 0;
    this.endTime = 0;
    this.startMemory = 0;
    this.endMemory = 0;
    this.peakMemory = 0;

    if (this.memoryInterval) {
      clearInterval(this.memoryInterval);
      this.memoryInterval = undefined;
    }
  }
}

/**
 * 基准测试运行器实现
 */
export class BenchmarkRunnerImpl implements BenchmarkRunner {
  private monitor: PerformanceMonitor;

  constructor() {
    this.monitor = new PerformanceMonitorImpl();
  }

  /**
   * 运行单个基准测试
   */
  async runTest(test: BenchmarkTest, config: BenchmarkConfig = {}): Promise<BenchmarkResult> {
    const finalConfig = this.mergeConfig(config, test.config);
    const latencies: number[] = [];

    // 执行setup
    if (test.setup) {
      await test.setup(finalConfig);
    }

    try {
      // 预热运行
      if (finalConfig.warmupRuns && finalConfig.warmupRuns > 0) {
        for (let i = 0; i < finalConfig.warmupRuns; i++) {
          await test.test(finalConfig);
        }
      }

      // 正式测试运行
      const runs = finalConfig.runs || 1;
      let totalExecutionTime = 0;
      let totalMemoryUsage = 0;
      let operations = 0;

      for (let i = 0; i < runs; i++) {
        this.monitor.reset();
        this.monitor.start();

        const runStartTime = performance.now();
        const result = await this.executeWithTimeout(test.test(finalConfig), finalConfig.timeout);
        const runEndTime = performance.now();

        const metrics = this.monitor.stop();
        const latency = runEndTime - runStartTime;

        latencies.push(latency);
        totalExecutionTime += metrics.executionTime;
        totalMemoryUsage += metrics.memoryDelta;

        if (result && typeof result === 'object' && 'operations' in result) {
          operations += result.operations || 1;
        } else {
          operations += 1;
        }
      }

      // 计算统计指标
      const averageExecutionTime = totalExecutionTime / runs;
      const averageMemoryUsage = totalMemoryUsage / runs;
      const averageLatency = latencies.reduce((sum, l) => sum + l, 0) / latencies.length;

      latencies.sort((a, b) => a - b);
      const minLatency = latencies[0];
      const maxLatency = latencies[latencies.length - 1];
      const p95Index = Math.floor(latencies.length * 0.95);
      const p99Index = Math.floor(latencies.length * 0.99);
      const p95Latency = latencies[p95Index];
      const p99Latency = latencies[p99Index];

      return {
        name: test.name,
        description: test.description,
        executionTime: averageExecutionTime,
        memoryUsage: Math.max(0, averageMemoryUsage),
        operations,
        operationsPerSecond: operations / (averageExecutionTime / 1000),
        averageLatency,
        minLatency,
        maxLatency,
        p95Latency,
        p99Latency,
        dataSize: finalConfig.dataGeneration?.size || 0,
        timestamp: new Date(),
      };
    } finally {
      // 执行teardown
      if (test.teardown) {
        await test.teardown(finalConfig);
      }
    }
  }

  /**
   * 运行测试套件
   */
  async runSuite(suite: BenchmarkSuite): Promise<BenchmarkResult[]> {
    const results: BenchmarkResult[] = [];

    console.log(`\n运行测试套件: ${suite.name}`);
    console.log(`描述: ${suite.description}\n`);

    for (const test of suite.benchmarks) {
      try {
        console.log(`执行测试: ${test.name}...`);
        const result = await this.runTest(test, suite.config);
        results.push(result);
        console.log(`✓ ${test.name} - ${result.executionTime.toFixed(2)}ms`);
      } catch (error) {
        const errMsg = error instanceof Error ? (error.stack ?? error.message) : String(error);
        console.error(`✗ ${test.name} - 测试失败: ${errMsg}`);
        // 创建失败结果
        results.push({
          name: test.name,
          description: test.description,
          executionTime: 0,
          memoryUsage: 0,
          operations: 0,
          operationsPerSecond: 0,
          averageLatency: 0,
          minLatency: 0,
          maxLatency: 0,
          p95Latency: 0,
          p99Latency: 0,
          dataSize: 0,
          timestamp: new Date(),
          metrics: { error: 1 },
        });
      }
    }

    return results;
  }

  /**
   * 运行所有测试套件
   */
  async runAll(suites: BenchmarkSuite[]): Promise<BenchmarkReport> {
    const startTime = Date.now();
    const allResults: BenchmarkResult[] = [];

    console.log(`开始运行 ${suites.length} 个测试套件...\n`);

    // 运行所有套件
    for (const suite of suites) {
      const results = await this.runSuite(suite);
      allResults.push(...results);
    }

    const endTime = Date.now();

    // 生成报告
    const environment = this.getEnvironmentInfo();
    const summary = this.generateSummary(allResults, endTime - startTime);

    return {
      timestamp: new Date(),
      environment,
      results: allResults,
      summary,
    };
  }

  /**
   * 合并配置
   */
  private mergeConfig(config1?: BenchmarkConfig, config2?: BenchmarkConfig): BenchmarkConfig {
    return {
      warmupRuns: 3,
      runs: 5,
      timeout: 30000,
      collectMemoryUsage: true,
      collectLatencyStats: true,
      ...config1,
      ...config2,
    };
  }

  /**
   * 带超时的执行
   */
  private async executeWithTimeout<T>(promise: Promise<T> | T, timeout?: number): Promise<T> {
    if (!timeout) {
      return Promise.resolve(promise);
    }

    return Promise.race([
      Promise.resolve(promise),
      new Promise<T>((_, reject) => {
        setTimeout(() => reject(new Error(`测试超时 (${timeout}ms)`)), timeout);
      }),
    ]);
  }

  /**
   * 获取环境信息
   */
  private getEnvironmentInfo(): EnvironmentInfo {
    // 读取内存信息用于整体概览（当前未直接使用返回值）

    return {
      nodeVersion: process.version,
      platform: `${os.type()} ${os.release()}`,
      arch: os.arch(),
      totalMemory: os.totalmem(),
      cpuCores: os.cpus().length,
      timestamp: new Date(),
    };
  }

  /**
   * 生成测试摘要
   */
  private generateSummary(results: BenchmarkResult[], totalTime: number): BenchmarkSummary {
    const passedTests = results.filter((r) => !r.metrics?.error).length;
    const failedTests = results.length - passedTests;

    let fastestTest = '';
    let slowestTest = '';
    let minTime = Infinity;
    let maxTime = 0;
    let totalExecutionTime = 0;
    let peakMemoryUsage = 0;

    for (const result of results) {
      if (result.metrics?.error) continue;

      totalExecutionTime += result.executionTime;

      if (result.executionTime < minTime) {
        minTime = result.executionTime;
        fastestTest = result.name;
      }

      if (result.executionTime > maxTime) {
        maxTime = result.executionTime;
        slowestTest = result.name;
      }

      if (result.memoryUsage > peakMemoryUsage) {
        peakMemoryUsage = result.memoryUsage;
      }
    }

    return {
      totalTests: results.length,
      passedTests,
      failedTests,
      totalExecutionTime: totalTime,
      fastestTest,
      slowestTest,
      averageExecutionTime: passedTests > 0 ? totalExecutionTime / passedTests : 0,
      peakMemoryUsage,
    };
  }
}

/**
 * 基准测试工具类
 */
export class BenchmarkUtils {
  /**
   * 格式化字节数
   */
  static formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';

    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  /**
   * 格式化时间
   */
  static formatTime(ms: number): string {
    if (ms < 1000) {
      return `${ms.toFixed(2)}ms`;
    } else if (ms < 60000) {
      return `${(ms / 1000).toFixed(2)}s`;
    } else {
      const minutes = Math.floor(ms / 60000);
      const seconds = ((ms % 60000) / 1000).toFixed(2);
      return `${minutes}m ${seconds}s`;
    }
  }

  /**
   * 格式化数字
   */
  static formatNumber(num: number): string {
    if (num >= 1000000) {
      return (num / 1000000).toFixed(2) + 'M';
    } else if (num >= 1000) {
      return (num / 1000).toFixed(2) + 'K';
    } else {
      return num.toFixed(2);
    }
  }

  /**
   * 计算性能变化百分比
   */
  static calculateChangePercent(current: number, baseline: number): number {
    if (baseline === 0) return current > 0 ? 100 : 0;
    return ((current - baseline) / baseline) * 100;
  }

  /**
   * 判断性能是否退化
   */
  static isRegression(changePercent: number, threshold: number): boolean {
    return changePercent > threshold;
  }

  /**
   * 生成随机数据
   */
  static generateRandomString(
    length: number,
    charset: string = 'abcdefghijklmnopqrstuvwxyz',
  ): string {
    let result = '';
    for (let i = 0; i < length; i++) {
      result += charset.charAt(Math.floor(Math.random() * charset.length));
    }
    return result;
  }

  /**
   * 生成随机整数
   */
  static generateRandomInt(min: number, max: number): number {
    return Math.floor(Math.random() * (max - min + 1)) + min;
  }

  /**
   * 创建延迟Promise
   */
  static delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * 测量异步函数执行时间
   */
  static async measureAsync<T>(fn: () => Promise<T>): Promise<{ result: T; time: number }> {
    const start = performance.now();
    const result = await fn();
    const time = performance.now() - start;
    return { result, time };
  }

  /**
   * 测量同步函数执行时间
   */
  static measure<T>(fn: () => T): { result: T; time: number } {
    const start = performance.now();
    const result = fn();
    const time = performance.now() - start;
    return { result, time };
  }
}

/**
 * 基准测试装饰器
 */
export function benchmark(name: string, description: string = '', config?: BenchmarkConfig) {
  // 未使用的参数标记
  void description;
  void config;
  return function (target: unknown, propertyKey: string, descriptor: PropertyDescriptor) {
    const originalMethod = descriptor.value as (...args: unknown[]) => Promise<unknown>;

    descriptor.value = async function (...args: unknown[]) {
      const monitor = new PerformanceMonitorImpl();

      monitor.start();
      const result = await Promise.resolve(originalMethod.apply(this, args));
      const metrics = monitor.stop();

      console.log(
        `基准测试 [${name}]: ${metrics.executionTime.toFixed(2)}ms, 内存: ${BenchmarkUtils.formatBytes(metrics.memoryDelta)}`,
      );

      return result;
    };

    return descriptor;
  };
}

/**
 * 内存使用监控装饰器
 */
export function memoryMonitor(
  target: unknown,
  propertyKey: string,
  descriptor: PropertyDescriptor,
) {
  const originalMethod = descriptor.value as (...args: unknown[]) => Promise<unknown>;

  descriptor.value = async function (...args: unknown[]) {
    const beforeMemory = process.memoryUsage().heapUsed;
    const result = await Promise.resolve(originalMethod.apply(this, args));
    const afterMemory = process.memoryUsage().heapUsed;
    const memoryDelta = afterMemory - beforeMemory;

    if (memoryDelta > 1024 * 1024) {
      // 超过1MB才警告
      console.warn(`内存使用警告 [${propertyKey}]: +${BenchmarkUtils.formatBytes(memoryDelta)}`);
    }

    return result;
  };

  return descriptor;
}

/**
 * 性能阈值检查装饰器
 */
export function performanceThreshold(maxTimeMs: number) {
  return function (target: unknown, propertyKey: string, descriptor: PropertyDescriptor) {
    const originalMethod = descriptor.value as (...args: unknown[]) => Promise<unknown>;

    descriptor.value = async function (...args: unknown[]) {
      const start = performance.now();
      const result = await Promise.resolve(originalMethod.apply(this, args));
      const executionTime = performance.now() - start;

      if (executionTime > maxTimeMs) {
        console.warn(
          `性能阈值警告 [${propertyKey}]: ${executionTime.toFixed(2)}ms > ${maxTimeMs}ms`,
        );
      }

      return result;
    };

    return descriptor;
  };
}
