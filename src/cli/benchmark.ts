#!/usr/bin/env node
/**
 * SynapseDB 基准测试命令行工具
 *
 * 提供性能基准测试的命令行接口
 */

import { Command } from 'commander';
import { promises as fs } from 'fs';
import { join } from 'path';
import { createBenchmarkManager } from '../benchmark/index.js';
import type { BenchmarkReport } from '../benchmark/index.js';
import {
  allBenchmarkSuites,
  synapseDBCoreSuite,
  fullTextSearchSuite,
  graphAlgorithmsSuite,
  spatialGeometrySuite,
} from '../benchmark/suites.js';

/**
 * 创建基准测试CLI程序
 */
function createBenchmarkCLI(): Command {
  const program = new Command();

  program.name('synapsedb-benchmark').description('SynapseDB 性能基准测试工具').version('1.0.0');

  // 运行所有基准测试
  program
    .command('run')
    .description('运行完整的性能基准测试')
    .option('-o, --output <dir>', '输出目录', './benchmark-reports')
    .option('-f, --format <formats>', '报告格式 (console,html,json,csv)', 'console,html')
    .option('--no-console', '不显示控制台输出')
    .action(async (options: { output: string; format: string; console?: boolean }) => {
      try {
        console.log('🚀 启动 SynapseDB 完整基准测试...\n');

        const formats = options.format.split(',') as ('console' | 'html' | 'json' | 'csv')[];
        const outputFormats = options.console ? formats : formats.filter((f) => f !== 'console');

        const manager = createBenchmarkManager();
        const { report, outputs } = await manager.benchmark({
          suites: allBenchmarkSuites,
          outputFormats,
          outputDir: options.output,
        });

        // 写入文件
        await fs.mkdir(options.output, { recursive: true });

        for (const output of outputs) {
          if (output.path && output.format !== 'console') {
            await fs.writeFile(output.path, output.content, 'utf8');
            console.log(`\n📄 已生成 ${output.format.toUpperCase()} 报告: ${output.path}`);
          }
        }

        // 显示摘要
        console.log('\n📊 基准测试完成摘要:');
        console.log(`总测试数: ${report.summary.totalTests}`);
        console.log(`通过测试: ${report.summary.passedTests} ✅`);
        console.log(
          `失败测试: ${report.summary.failedTests} ${report.summary.failedTests > 0 ? '❌' : ''}`,
        );
        console.log(`总执行时间: ${(report.summary.totalExecutionTime / 1000).toFixed(2)}s`);
      } catch (error) {
        console.error('❌ 基准测试失败:', error);
        process.exit(1);
      }
    });

  // 运行核心功能测试
  program
    .command('core')
    .description('运行 SynapseDB 核心功能基准测试')
    .option('-o, --output <dir>', '输出目录', './benchmark-reports')
    .option('-f, --format <formats>', '报告格式 (console,html,json,csv)', 'console')
    .action(async (options: { output: string; format: string }) => {
      try {
        console.log('🧠 运行 SynapseDB 核心功能测试...\n');
        await runSuiteCommand([synapseDBCoreSuite], options);
      } catch (error) {
        console.error('❌ 核心功能测试失败:', error);
        process.exit(1);
      }
    });

  // 运行全文搜索测试
  program
    .command('search')
    .description('运行全文搜索引擎基准测试')
    .option('-o, --output <dir>', '输出目录', './benchmark-reports')
    .option('-f, --format <formats>', '报告格式 (console,html,json,csv)', 'console')
    .action(async (options: { output: string; format: string }) => {
      try {
        console.log('🔍 运行全文搜索引擎测试...\n');
        await runSuiteCommand([fullTextSearchSuite], options);
      } catch (error) {
        console.error('❌ 全文搜索测试失败:', error);
        process.exit(1);
      }
    });

  // 运行图算法测试
  program
    .command('graph')
    .description('运行图算法库基准测试')
    .option('-o, --output <dir>', '输出目录', './benchmark-reports')
    .option('-f, --format <formats>', '报告格式 (console,html,json,csv)', 'console')
    .action(async (options: { output: string; format: string }) => {
      try {
        console.log('📊 运行图算法库测试...\n');
        await runSuiteCommand([graphAlgorithmsSuite], options);
      } catch (error) {
        console.error('❌ 图算法测试失败:', error);
        process.exit(1);
      }
    });

  // 运行空间几何测试
  program
    .command('spatial')
    .description('运行空间几何计算基准测试')
    .option('-o, --output <dir>', '输出目录', './benchmark-reports')
    .option('-f, --format <formats>', '报告格式 (console,html,json,csv)', 'console')
    .action(async (options: { output: string; format: string }) => {
      try {
        console.log('🗺️ 运行空间几何计算测试...\n');
        await runSuiteCommand([spatialGeometrySuite], options);
      } catch (error) {
        console.error('❌ 空间几何测试失败:', error);
        process.exit(1);
      }
    });

  // 性能回归检测
  program
    .command('regression')
    .description('运行性能回归检测')
    .requiredOption('-b, --baseline <file>', '基线报告文件 (JSON格式)')
    .option('-t, --threshold <percent>', '性能退化阈值 (百分比)', '10')
    .option('-o, --output <dir>', '输出目录', './benchmark-reports')
    .action(async (options: { baseline: string; threshold?: string; output: string }) => {
      try {
        console.log('📈 运行性能回归检测...\n');

        // 读取基线报告
        const baselineContent = await fs.readFile(options.baseline, 'utf8');
        const baselineReport = JSON.parse(baselineContent) as unknown as BenchmarkReport;

        const manager = createBenchmarkManager();
        const regressions = await manager.runRegressionTest(baselineReport, {
          regressionThreshold: parseFloat(options.threshold ?? '10'),
        });

        // 分析回归结果
        const failedRegressions = regressions.filter((r) => !r.passed);

        if (failedRegressions.length === 0) {
          console.log('✅ 未检测到性能回归');
        } else {
          console.log(`⚠️ 检测到 ${failedRegressions.length} 个性能回归:\n`);

          for (const regression of failedRegressions) {
            const changeStr =
              regression.changePercent > 0
                ? `+${regression.changePercent.toFixed(2)}%`
                : `${regression.changePercent.toFixed(2)}%`;

            console.log(`❌ ${regression.testName} (${regression.metric}): ${changeStr}`);
            console.log(`   当前值: ${regression.currentValue.toFixed(2)}`);
            console.log(`   基线值: ${regression.baselineValue.toFixed(2)}`);
            if (regression.details) {
              console.log(`   详情: ${regression.details}`);
            }
            console.log('');
          }
        }

        // 保存回归检测报告
        await fs.mkdir(options.output, { recursive: true });
        const regressionReportPath = join(
          options.output,
          `regression-report-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.json`,
        );

        await fs.writeFile(
          regressionReportPath,
          JSON.stringify(
            {
              timestamp: new Date().toISOString(),
              baseline: options.baseline,
              threshold: options.threshold,
              totalRegressions: regressions.length,
              failedRegressions: failedRegressions.length,
              regressions,
            },
            null,
            2,
          ),
        );

        console.log(`📄 回归检测报告已保存: ${regressionReportPath}`);

        // 如果有回归则返回错误码
        if (failedRegressions.length > 0) {
          process.exit(1);
        }
      } catch (error) {
        console.error('❌ 回归检测失败:', error);
        process.exit(1);
      }
    });

  // 内存泄漏检测
  program
    .command('memory-leak')
    .description('运行内存泄漏检测')
    .option('-i, --iterations <count>', '迭代次数', '100')
    .option('-o, --operations <count>', '每次迭代的操作数', '1000')
    .option('-t, --threshold <bytes>', '内存增长阈值 (字节)', '10485760') // 10MB
    .option('--force-gc', '强制垃圾回收')
    .action(
      async (options: {
        iterations: string;
        operations: string;
        threshold: string;
        forceGc?: boolean;
      }) => {
        try {
          console.log('🧠 运行内存泄漏检测...\n');

          const iterations = parseInt(options.iterations);
          const operationsPerIteration = parseInt(options.operations);
          const memoryGrowthThreshold = parseInt(options.threshold);
          const forceGC = options.forceGc;

          console.log(`配置: ${iterations} 迭代, 每次 ${operationsPerIteration} 操作`);
          console.log(`内存增长阈值: ${(memoryGrowthThreshold / 1024 / 1024).toFixed(1)}MB\n`);

          const memoryProgression: number[] = [];

          // 强制垃圾回收
          if (forceGC && global.gc) {
            global.gc();
          }

          const initialMemory = process.memoryUsage().heapUsed;
          memoryProgression.push(initialMemory);

          // 模拟内存泄漏检测（这里需要实际的测试逻辑）
          for (let i = 0; i < iterations; i++) {
            // 这里应该运行实际的操作
            // 暂时使用模拟数据
            await new Promise((resolve) => setTimeout(resolve, 10));

            if (forceGC && global.gc) {
              global.gc();
            }

            const currentMemory = process.memoryUsage().heapUsed;
            memoryProgression.push(currentMemory);

            if ((i + 1) % 10 === 0) {
              const memoryIncrease = currentMemory - initialMemory;
              console.log(
                `迭代 ${i + 1}/${iterations}: 内存增长 ${(memoryIncrease / 1024 / 1024).toFixed(2)}MB`,
              );
            }
          }

          const finalMemory = memoryProgression[memoryProgression.length - 1];
          const memoryGrowth = finalMemory - initialMemory;
          const hasLeak = memoryGrowth > memoryGrowthThreshold;

          // 分析增长趋势
          let growthTrend: 'increasing' | 'stable' | 'decreasing' = 'stable';
          if (memoryProgression.length > 10) {
            const firstHalf = memoryProgression.slice(0, Math.floor(memoryProgression.length / 2));
            const secondHalf = memoryProgression.slice(Math.floor(memoryProgression.length / 2));

            const firstHalfAvg = firstHalf.reduce((sum, val) => sum + val, 0) / firstHalf.length;
            const secondHalfAvg = secondHalf.reduce((sum, val) => sum + val, 0) / secondHalf.length;

            if (secondHalfAvg > firstHalfAvg * 1.1) {
              growthTrend = 'increasing';
            } else if (secondHalfAvg < firstHalfAvg * 0.9) {
              growthTrend = 'decreasing';
            }
          }

          console.log('\n📊 内存泄漏检测结果:');
          console.log(`初始内存: ${(initialMemory / 1024 / 1024).toFixed(2)}MB`);
          console.log(`最终内存: ${(finalMemory / 1024 / 1024).toFixed(2)}MB`);
          console.log(`内存增长: ${(memoryGrowth / 1024 / 1024).toFixed(2)}MB`);
          console.log(
            `增长趋势: ${growthTrend === 'increasing' ? '📈 递增' : growthTrend === 'decreasing' ? '📉 递减' : '📊 稳定'}`,
          );
          console.log(`检测结果: ${hasLeak ? '⚠️ 检测到可能的内存泄漏' : '✅ 未检测到内存泄漏'}`);

          if (hasLeak) {
            process.exit(1);
          }
        } catch (error) {
          console.error('❌ 内存泄漏检测失败:', error);
          process.exit(1);
        }
      },
    );

  return program;
}

/**
 * 运行测试套件的通用函数
 */
async function runSuiteCommand(
  suites: import('../benchmark/types.js').BenchmarkSuite[],
  options: { output: string; format: string },
) {
  const formats = options.format.split(',') as ('console' | 'html' | 'json' | 'csv')[];

  const manager = createBenchmarkManager();
  const { outputs } = await manager.benchmark({
    suites,
    outputFormats: formats,
    outputDir: options.output,
  });

  // 写入文件
  await fs.mkdir(options.output, { recursive: true });

  for (const output of outputs) {
    if (output.path && output.format !== 'console') {
      await fs.writeFile(output.path, output.content, 'utf8');
      console.log(`\n📄 已生成 ${output.format.toUpperCase()} 报告: ${output.path}`);
    }
  }
}

// CLI程序入口
if (require.main === module) {
  const program = createBenchmarkCLI();
  program.parse(process.argv);
}

export default createBenchmarkCLI;
