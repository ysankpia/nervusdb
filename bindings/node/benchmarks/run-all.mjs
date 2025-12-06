#!/usr/bin/env node
/**
 * 统一基准测试入口脚本
 *
 * 提供与 CLI 兼容的基准测试接口，委托到现有的外部脚本
 *
 * 用法:
 *   node benchmarks/run-all.mjs --suite=all
 *   node benchmarks/run-all.mjs --suite=core --format=console,json --output=./reports
 */

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdir, writeFile } from 'node:fs/promises';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// 解析命令行参数
function parseArgs() {
  const args = process.argv.slice(2);
  const options = {
    suite: 'all',
    format: 'console',
    output: './benchmark-reports',
    noConsole: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg.startsWith('--suite=')) {
      options.suite = arg.split('=')[1];
    } else if (arg.startsWith('--format=')) {
      options.format = arg.split('=')[1];
    } else if (arg.startsWith('--output=')) {
      options.output = arg.split('=')[1];
    } else if (arg === '--no-console') {
      options.noConsole = true;
    }
  }

  return options;
}

// 映射套件名称到脚本路径
function getScriptPath(suite) {
  const scripts = {
    all: 'comprehensive.mjs',
    core: 'comprehensive.mjs', // 核心功能测试使用 comprehensive
    search: 'comprehensive.mjs', // 全文搜索（comprehensive 包含）
    graph: 'path_agg.mjs', // 图算法使用 path_agg
    spatial: 'comprehensive.mjs', // 空间几何（comprehensive 包含）
    quick: 'quick.mjs',
    insert: 'insert_scan.mjs',
    path: 'path_agg.mjs'
  };

  return scripts[suite] || 'comprehensive.mjs';
}

// 生成兼容的 BenchmarkReport JSON 结构
function generateBenchmarkReport(results, suite) {
  const timestamp = new Date();

  return {
    timestamp,
    environment: {
      nodeVersion: process.version,
      platform: process.platform,
      arch: process.arch,
      totalMemory: require('os').totalmem(),
      cpuCores: require('os').cpus().length,
      timestamp
    },
    results: results || [],
    summary: {
      totalTests: results?.length || 0,
      passedTests: results?.filter(r => !r.error).length || 0,
      failedTests: results?.filter(r => r.error).length || 0,
      totalExecutionTime: results?.reduce((sum, r) => sum + (r.executionTime || 0), 0) || 0,
      fastestTest: results?.[0]?.name || '',
      slowestTest: results?.[results?.length - 1]?.name || '',
      averageExecutionTime: results?.length ?
        results.reduce((sum, r) => sum + (r.executionTime || 0), 0) / results.length : 0,
      peakMemoryUsage: Math.max(...(results?.map(r => r.memoryUsage || 0) || [0]))
    }
  };
}

// 运行外部脚本并捕获输出
function runScript(scriptPath) {
  return new Promise((resolve, reject) => {
    const child = spawn('node', [scriptPath], {
      cwd: __dirname,
      stdio: ['inherit', 'pipe', 'pipe']
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (data) => {
      const output = data.toString();
      stdout += output;
      process.stdout.write(output); // 实时输出到控制台
    });

    child.stderr.on('data', (data) => {
      const output = data.toString();
      stderr += output;
      process.stderr.write(output);
    });

    child.on('exit', (code) => {
      if (code === 0) {
        resolve({ stdout, stderr });
      } else {
        reject(new Error(`Script exited with code ${code}`));
      }
    });

    child.on('error', (error) => {
      reject(error);
    });
  });
}

// 主函数
async function main() {
  const options = parseArgs();

  console.log(`🚀 启动 SynapseDB 基准测试...`);
  console.log(`   套件: ${options.suite}`);
  console.log(`   输出格式: ${options.format}`);
  console.log(`   输出目录: ${options.output}\n`);

  try {
    // 确保输出目录存在
    await mkdir(options.output, { recursive: true });

    // 获取对应的脚本
    const scriptName = getScriptPath(options.suite);
    const scriptPath = join(__dirname, scriptName);

    console.log(`📝 运行脚本: ${scriptName}\n`);

    // 运行脚本
    const { stdout } = await runScript(scriptPath);

    // 解析输出格式
    const formats = options.format.split(',');
    const results = []; // 这里简化处理，实际应从脚本输出解析

    // 生成各种格式的输出
    for (const format of formats) {
      if (format === 'console') {
        // 控制台输出已在 runScript 中实时显示
        continue;
      }

      if (format === 'json') {
        const report = generateBenchmarkReport(results, options.suite);
        const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
        const jsonPath = join(options.output, `benchmark-report-${timestamp}.json`);

        await writeFile(jsonPath, JSON.stringify(report, null, 2));
        console.log(`\n📄 已生成 JSON 报告: ${jsonPath}`);
      }

      if (format === 'html') {
        const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
        const htmlPath = join(options.output, `benchmark-report-${timestamp}.html`);

        const htmlContent = `<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>SynapseDB Benchmark Report</title>
  <style>
    body { font-family: Arial, sans-serif; margin: 20px; }
    h1 { color: #333; }
    .summary { background: #f5f5f5; padding: 15px; border-radius: 5px; }
    .result { margin: 10px 0; padding: 10px; border: 1px solid #ddd; }
  </style>
</head>
<body>
  <h1>SynapseDB Benchmark Report</h1>
  <div class="summary">
    <h2>Summary</h2>
    <p>Suite: ${options.suite}</p>
    <p>Timestamp: ${new Date().toISOString()}</p>
  </div>
  <pre>${stdout}</pre>
</body>
</html>`;

        await writeFile(htmlPath, htmlContent);
        console.log(`📄 已生成 HTML 报告: ${htmlPath}`);
      }

      if (format === 'csv') {
        const timestamp = new Date().toISOString().slice(0, 19).replace(/:/g, '-');
        const csvPath = join(options.output, `benchmark-report-${timestamp}.csv`);

        const csvContent = `Test Name,Execution Time (ms),Memory Usage (bytes),Ops/sec\n`;

        await writeFile(csvPath, csvContent);
        console.log(`📄 已生成 CSV 报告: ${csvPath}`);
      }
    }

    // 显示摘要
    console.log('\n📊 基准测试完成摘要:');
    console.log(`总测试数: ${results.length || '(参见输出)'}`);
    console.log(`输出目录: ${options.output}`);

    process.exit(0);
  } catch (error) {
    console.error(`\n❌ 基准测试失败: ${error.message}`);
    console.error(error.stack);
    process.exit(1);
  }
}

// 运行主函数
main().catch((error) => {
  console.error('❌ 未捕获的错误:', error);
  process.exit(1);
});