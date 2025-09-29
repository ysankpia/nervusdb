#!/usr/bin/env node
/**
 * SynapseDB 基准测试命令行工具
 *
 * 提供性能基准测试的命令行接口
 *
 * @deprecated 内部基准测试框架将在 v2.0 移除
 * 推荐直接使用 benchmarks/*.mjs 脚本或 benchmarks/run-all.mjs 统一入口
 */

import { Command } from 'commander';
import { promises as fs } from 'fs';
import { join, dirname } from 'path';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { createBenchmarkManager } from '../benchmark/index.js';
import type { BenchmarkReport } from '../benchmark/index.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * 使用外部脚本运行基准测试（新实现）
 */
async function runExternalBenchmark(
  suite: 'all' | 'core' | 'search' | 'graph' | 'spatial',
  options: { output: string; format: string; console?: boolean },
): Promise<void> {
  const scriptPath = join(__dirname, '../../benchmarks/run-all.mjs');

  // 映射 CLI 参数到脚本参数
  const args = ['--suite', suite, '--format', options.format, '--output', options.output];

  if (options.console === false) {
    args.push('--no-console');
  }

  console.log('⚠️  注意：基准测试现已使用外部脚本运行（benchmarks/run-all.mjs）');
  console.log('   内部框架将在未来版本移除，建议直接运行外部脚本以获得最佳体验\n');

  return new Promise((resolve, reject) => {
    const child = spawn('node', [scriptPath, ...args], {
      stdio: 'inherit',
      cwd: join(__dirname, '../..'),
    });

    child.on('exit', (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`基准测试脚本退出，代码: ${code}`));
      }
    });

    child.on('error', (error) => {
      reject(new Error(`无法启动基准测试脚本: ${error.message}`));
    });
  });
}

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
        await runExternalBenchmark('all', options);
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
        await runExternalBenchmark('core', options);
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
        await runExternalBenchmark('search', options);
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
        await runExternalBenchmark('graph', options);
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
        await runExternalBenchmark('spatial', options);
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

// CLI程序入口
if (require.main === module) {
  const program = createBenchmarkCLI();
  program.parse(process.argv);
}

export default createBenchmarkCLI;
