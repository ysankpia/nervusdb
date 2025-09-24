#!/usr/bin/env node
/**
 * 性能基准测试框架 - v1.1 里程碑要求
 * 提供标准化的基准测试接口和结果验证
 */

/**
 * 单个基准测试配置
 */
export class Benchmark {
  constructor(name, description, setupFn, testFn, teardownFn, validator) {
    this.name = name;
    this.description = description;
    this.setup = setupFn || (() => Promise.resolve());
    this.test = testFn;
    this.teardown = teardownFn || (() => Promise.resolve());
    this.validator = validator; // 可选的结果验证函数
  }
}

/**
 * 基准测试套件
 */
export class BenchmarkSuite {
  constructor(name, description) {
    this.name = name;
    this.description = description;
    this.benchmarks = [];
    this.globalSetup = null;
    this.globalTeardown = null;
  }

  addBenchmark(benchmark) {
    this.benchmarks.push(benchmark);
    return this;
  }

  setGlobalSetup(fn) {
    this.globalSetup = fn;
    return this;
  }

  setGlobalTeardown(fn) {
    this.globalTeardown = fn;
    return this;
  }
}

/**
 * 基准测试结果
 */
export class BenchmarkResult {
  constructor(name, duration, memoryUsage, result, error = null) {
    this.name = name;
    this.duration = duration; // 毫秒
    this.memoryUsage = memoryUsage; // 字节
    this.result = result;
    this.error = error;
    this.throughput = null; // ops/sec（如果适用）
    this.validated = false;
  }

  setThroughput(ops, durationMs) {
    this.throughput = Math.round((ops * 1000) / durationMs);
    return this;
  }

  setValidated(isValid) {
    this.validated = isValid;
    return this;
  }
}

/**
 * 基准测试运行器
 */
export class BenchmarkRunner {
  constructor(options = {}) {
    this.warmupRuns = options.warmupRuns || 3; // 预热次数
    this.measurementRuns = options.measurementRuns || 5; // 测量次数
    this.verbose = options.verbose || false;
    this.collectGC = options.collectGC !== false; // 默认开启GC
  }

  /**
   * 运行单个基准测试套件
   */
  async run(suite) {
    console.log(`\n🚀 运行基准套件: ${suite.name}`);
    if (suite.description) {
      console.log(`   ${suite.description}`);
    }

    const suiteResults = {
      suite: suite.name,
      description: suite.description,
      startTime: Date.now(),
      endTime: null,
      totalDuration: null,
      results: [],
      summary: {},
    };

    try {
      // 全局设置
      if (suite.globalSetup) {
        await suite.globalSetup();
      }

      // 运行每个基准测试
      for (const benchmark of suite.benchmarks) {
        try {
          const result = await this.runSingleBenchmark(benchmark);
          suiteResults.results.push(result);
        } catch (error) {
          console.error(`❌ 基准测试 ${benchmark.name} 失败:`, error.message);
          suiteResults.results.push(
            new BenchmarkResult(benchmark.name, 0, 0, null, error),
          );
        }
      }

      // 全局清理
      if (suite.globalTeardown) {
        await suite.globalTeardown();
      }

      suiteResults.endTime = Date.now();
      suiteResults.totalDuration = suiteResults.endTime - suiteResults.startTime;

      // 生成摘要
      this.generateSummary(suiteResults);

      return suiteResults;
    } catch (error) {
      console.error(`❌ 套件 ${suite.name} 执行失败:`, error);
      suiteResults.endTime = Date.now();
      suiteResults.totalDuration = suiteResults.endTime - suiteResults.startTime;
      return suiteResults;
    }
  }

  /**
   * 运行单个基准测试
   */
  async runSingleBenchmark(benchmark) {
    console.log(`\n📊 ${benchmark.name}`);
    if (benchmark.description) {
      console.log(`   ${benchmark.description}`);
    }

    const durations = [];
    const memoryUsages = [];
    let lastResult = null;

    try {
      // 执行设置
      await benchmark.setup();

      // 预热运行
      if (this.verbose) console.log(`   🔥 预热 ${this.warmupRuns} 次...`);
      for (let i = 0; i < this.warmupRuns; i++) {
        if (this.collectGC && global.gc) {
          global.gc();
        }
        await benchmark.test();
      }

      // 测量运行
      if (this.verbose) console.log(`   📏 测量 ${this.measurementRuns} 次...`);
      for (let i = 0; i < this.measurementRuns; i++) {
        if (this.collectGC && global.gc) {
          global.gc();
        }

        const memBefore = process.memoryUsage().heapUsed;
        const timeBefore = performance.now();

        lastResult = await benchmark.test();

        const timeAfter = performance.now();
        const memAfter = process.memoryUsage().heapUsed;

        durations.push(timeAfter - timeBefore);
        memoryUsages.push(Math.max(0, memAfter - memBefore));
      }

      // 执行清理
      await benchmark.teardown();

      // 计算统计数据
      const avgDuration = durations.reduce((a, b) => a + b, 0) / durations.length;
      const avgMemory = memoryUsages.reduce((a, b) => a + b, 0) / memoryUsages.length;

      const result = new BenchmarkResult(
        benchmark.name,
        Math.round(avgDuration * 100) / 100, // 保留2位小数
        Math.round(avgMemory),
        lastResult,
      );

      // 验证结果（如果提供了验证器）
      if (benchmark.validator) {
        try {
          const isValid = await benchmark.validator(lastResult);
          result.setValidated(isValid);
          if (this.verbose) {
            console.log(`   ✅ 结果验证: ${isValid ? '通过' : '失败'}`);
          }
        } catch (validationError) {
          console.warn(`   ⚠️  结果验证失败: ${validationError.message}`);
          result.setValidated(false);
        }
      }

      // 输出结果
      console.log(`   ⏱️  平均耗时: ${result.duration.toFixed(2)}ms`);
      console.log(`   💾 平均内存: ${this.formatBytes(result.memoryUsage)}`);
      if (result.throughput) {
        console.log(`   🚄 吞吐量: ${result.throughput.toLocaleString()} ops/sec`);
      }

      return result;
    } catch (error) {
      console.error(`   ❌ 执行失败: ${error.message}`);
      try {
        await benchmark.teardown();
      } catch (teardownError) {
        console.error(`   ❌ 清理失败: ${teardownError.message}`);
      }
      throw error;
    }
  }

  /**
   * 生成测试摘要
   */
  generateSummary(suiteResults) {
    const successfulResults = suiteResults.results.filter((r) => !r.error);
    const failedResults = suiteResults.results.filter((r) => r.error);

    suiteResults.summary = {
      total: suiteResults.results.length,
      successful: successfulResults.length,
      failed: failedResults.length,
      totalTime: suiteResults.totalDuration,
      avgDuration:
        successfulResults.length > 0
          ? successfulResults.reduce((sum, r) => sum + r.duration, 0) / successfulResults.length
          : 0,
      totalMemory: successfulResults.reduce((sum, r) => sum + r.memoryUsage, 0),
      fastestTest: successfulResults.length > 0 ?
        successfulResults.reduce((min, r) => (r.duration < min.duration ? r : min)) : null,
      slowestTest: successfulResults.length > 0 ?
        successfulResults.reduce((max, r) => (r.duration > max.duration ? r : max)) : null,
    };

    console.log(`\n📈 基准测试摘要 - ${suiteResults.suite}`);
    console.log(`   总数: ${suiteResults.summary.total}`);
    console.log(`   成功: ${suiteResults.summary.successful}`);
    console.log(`   失败: ${suiteResults.summary.failed}`);
    console.log(`   总耗时: ${suiteResults.summary.totalTime}ms`);
    console.log(`   平均耗时: ${suiteResults.summary.avgDuration.toFixed(2)}ms`);
    console.log(`   总内存: ${this.formatBytes(suiteResults.summary.totalMemory)}`);

    if (suiteResults.summary.fastestTest) {
      console.log(`   最快: ${suiteResults.summary.fastestTest.name} (${suiteResults.summary.fastestTest.duration.toFixed(2)}ms)`);
    }
    if (suiteResults.summary.slowestTest) {
      console.log(`   最慢: ${suiteResults.summary.slowestTest.name} (${suiteResults.summary.slowestTest.duration.toFixed(2)}ms)`);
    }
  }

  /**
   * 格式化字节数显示
   */
  formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(Math.abs(bytes)) / Math.log(k));
    return (bytes / Math.pow(k, i)).toFixed(1) + ' ' + sizes[i];
  }

  /**
   * 生成JSON报告
   */
  generateJsonReport(suiteResults, outputPath = null) {
    const report = {
      ...suiteResults,
      generatedAt: new Date().toISOString(),
      nodeVersion: process.version,
      platform: process.platform,
      arch: process.arch,
      cpus: require('os').cpus().length,
      memory: process.memoryUsage(),
    };

    if (outputPath) {
      require('fs').writeFileSync(outputPath, JSON.stringify(report, null, 2));
      console.log(`\n📄 报告已生成: ${outputPath}`);
    }

    return report;
  }
}

/**
 * 性能指标验证工具
 */
export class PerformanceValidator {
  /**
   * 验证耗时是否在预期范围内
   */
  static timeWithin(maxMs) {
    return (result, duration) => duration <= maxMs;
  }

  /**
   * 验证内存使用是否在预期范围内
   */
  static memoryWithin(maxBytes) {
    return (result, duration, memory) => memory <= maxBytes;
  }

  /**
   * 验证结果数量
   */
  static resultCount(expectedCount) {
    return (result) => {
      if (Array.isArray(result)) {
        return result.length === expectedCount;
      }
      return result === expectedCount;
    };
  }

  /**
   * 验证结果包含特定内容
   */
  static resultContains(expectedContent) {
    return (result) => {
      if (Array.isArray(result)) {
        return result.some((item) =>
          JSON.stringify(item).includes(JSON.stringify(expectedContent))
        );
      }
      return JSON.stringify(result).includes(JSON.stringify(expectedContent));
    };
  }
}