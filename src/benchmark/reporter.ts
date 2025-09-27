/**
 * 基准测试报告生成器
 *
 * 提供多种格式的性能测试报告输出
 */

import { BenchmarkReport, BenchmarkResult, BenchmarkReporter } from './types.js';
import { BenchmarkUtils } from './runner.js';

/**
 * 基准测试报告生成器实现
 */
export class BenchmarkReporterImpl implements BenchmarkReporter {
  /**
   * 生成控制台报告
   */
  generateConsoleReport(report: BenchmarkReport): string {
    const lines: string[] = [];
    const { results, summary, environment, timestamp, regressions } = report;

    // 报告头部
    lines.push('');
    lines.push('═'.repeat(80));
    lines.push('🏆 SynapseDB 性能基准测试报告');
    lines.push('═'.repeat(80));
    lines.push(`测试时间: ${timestamp.toLocaleString()}`);
    lines.push('');

    // 环境信息
    lines.push('📊 测试环境信息');
    lines.push('─'.repeat(40));
    lines.push(`Node.js版本: ${environment.nodeVersion}`);
    lines.push(`操作系统: ${environment.platform}`);
    lines.push(`CPU架构: ${environment.arch}`);
    lines.push(`CPU核心数: ${environment.cpuCores}`);
    lines.push(`总内存: ${BenchmarkUtils.formatBytes(environment.totalMemory)}`);
    lines.push('');

    // 测试摘要
    lines.push('📈 测试摘要');
    lines.push('─'.repeat(40));
    lines.push(`总测试数: ${summary.totalTests}`);
    lines.push(`通过测试: ${summary.passedTests} ✅`);
    lines.push(`失败测试: ${summary.failedTests} ${summary.failedTests > 0 ? '❌' : ''}`);
    lines.push(`总执行时间: ${BenchmarkUtils.formatTime(summary.totalExecutionTime)}`);
    lines.push(`平均执行时间: ${BenchmarkUtils.formatTime(summary.averageExecutionTime)}`);
    lines.push(`最快测试: ${summary.fastestTest}`);
    lines.push(`最慢测试: ${summary.slowestTest}`);
    lines.push(`峰值内存: ${BenchmarkUtils.formatBytes(summary.peakMemoryUsage)}`);
    lines.push('');

    // 详细测试结果
    lines.push('📋 详细测试结果');
    lines.push('─'.repeat(80));

    const groupedResults = this.groupResultsBySuite(results);

    for (const [suiteName, suiteResults] of groupedResults) {
      lines.push('');
      lines.push(`📦 ${suiteName}`);
      lines.push('┌' + '─'.repeat(78) + '┐');
      lines.push(
        '│ 测试名称' +
          ' '.repeat(25) +
          '│ 执行时间' +
          ' '.repeat(5) +
          '│ 操作/秒' +
          ' '.repeat(5) +
          '│ 内存使用 │',
      );
      lines.push('├' + '─'.repeat(78) + '┤');

      for (const result of suiteResults) {
        const isError = result.metrics?.error;
        const status = isError ? '❌' : '✅';
        const name = this.truncateString(result.name, 32);
        const time = isError ? 'ERROR' : BenchmarkUtils.formatTime(result.executionTime);
        const ops = isError ? '-' : BenchmarkUtils.formatNumber(result.operationsPerSecond);
        const memory = isError ? '-' : BenchmarkUtils.formatBytes(result.memoryUsage);

        lines.push(
          `│ ${status} ${name.padEnd(30)} │ ${time.padEnd(12)} │ ${ops.padEnd(12)} │ ${memory.padEnd(9)} │`,
        );
      }

      lines.push('└' + '─'.repeat(78) + '┘');
    }

    // 性能回归检测
    if (regressions && regressions.length > 0) {
      lines.push('');
      lines.push('⚠️  性能回归检测');
      lines.push('─'.repeat(40));

      const failedRegressions = regressions.filter((r) => !r.passed);
      if (failedRegressions.length > 0) {
        lines.push(`检测到 ${failedRegressions.length} 个性能回归:`);
        for (const regression of failedRegressions) {
          const changeStr =
            regression.changePercent > 0
              ? `+${regression.changePercent.toFixed(2)}%`
              : `${regression.changePercent.toFixed(2)}%`;
          lines.push(`  ❌ ${regression.testName} (${regression.metric}): ${changeStr}`);
          if (regression.details) {
            lines.push(`     ${regression.details}`);
          }
        }
      } else {
        lines.push('✅ 未检测到性能回归');
      }
      lines.push('');
    }

    // 性能建议
    lines.push('💡 性能建议');
    lines.push('─'.repeat(40));
    const suggestions = this.generatePerformanceSuggestions(results);
    if (suggestions.length > 0) {
      suggestions.forEach((suggestion) => lines.push(`• ${suggestion}`));
    } else {
      lines.push('• 当前性能表现良好，无特殊建议');
    }
    lines.push('');

    lines.push('═'.repeat(80));
    return lines.join('\n');
  }

  /**
   * 生成JSON报告
   */
  generateJSONReport(report: BenchmarkReport): string {
    return JSON.stringify(report, null, 2);
  }

  /**
   * 生成HTML报告
   */
  generateHTMLReport(report: BenchmarkReport): string {
    const { results, summary, environment, timestamp, regressions } = report;
    const groupedResults = this.groupResultsBySuite(results);

    return `
<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SynapseDB 性能基准测试报告</title>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; margin: 0; padding: 20px; background: #f5f7fa; }
        .container { max-width: 1200px; margin: 0 auto; background: white; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
        .header { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 30px; border-radius: 8px 8px 0 0; }
        .header h1 { margin: 0; font-size: 28px; }
        .header .timestamp { opacity: 0.8; margin-top: 10px; }
        .content { padding: 30px; }
        .section { margin-bottom: 30px; }
        .section h2 { color: #333; border-bottom: 2px solid #eee; padding-bottom: 10px; }
        .stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; margin-bottom: 20px; }
        .stat-card { background: #f8f9fa; padding: 20px; border-radius: 6px; border-left: 4px solid #007bff; }
        .stat-value { font-size: 24px; font-weight: bold; color: #007bff; }
        .stat-label { color: #6c757d; margin-top: 5px; }
        .table { width: 100%; border-collapse: collapse; margin-top: 15px; }
        .table th, .table td { padding: 12px; text-align: left; border-bottom: 1px solid #dee2e6; }
        .table th { background-color: #f8f9fa; font-weight: 600; }
        .suite-header { background: #e3f2fd; padding: 15px; margin: 20px 0 10px 0; border-radius: 4px; font-weight: bold; }
        .status-success { color: #28a745; }
        .status-error { color: #dc3545; }
        .progress-bar { background: #e9ecef; height: 8px; border-radius: 4px; overflow: hidden; margin: 5px 0; }
        .progress-fill { background: linear-gradient(90deg, #28a745, #20c997); height: 100%; transition: width 0.3s ease; }
        .regression-item { padding: 10px; margin: 5px 0; border-left: 4px solid #dc3545; background: #fff5f5; border-radius: 0 4px 4px 0; }
        .chart-container { height: 300px; margin: 20px 0; }
        @media (max-width: 768px) { .stats-grid { grid-template-columns: 1fr; } .container { margin: 10px; } .content { padding: 15px; } }
    </style>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🏆 SynapseDB 性能基准测试报告</h1>
            <div class="timestamp">测试时间: ${timestamp.toLocaleString()}</div>
        </div>

        <div class="content">
            <!-- 环境信息 -->
            <div class="section">
                <h2>📊 测试环境信息</h2>
                <div class="stats-grid">
                    <div class="stat-card">
                        <div class="stat-value">${environment.nodeVersion}</div>
                        <div class="stat-label">Node.js 版本</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">${environment.cpuCores}</div>
                        <div class="stat-label">CPU 核心数</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">${BenchmarkUtils.formatBytes(environment.totalMemory)}</div>
                        <div class="stat-label">总内存</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">${environment.platform}</div>
                        <div class="stat-label">操作系统</div>
                    </div>
                </div>
            </div>

            <!-- 测试摘要 -->
            <div class="section">
                <h2>📈 测试摘要</h2>
                <div class="stats-grid">
                    <div class="stat-card">
                        <div class="stat-value">${summary.totalTests}</div>
                        <div class="stat-label">总测试数</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value status-success">${summary.passedTests}</div>
                        <div class="stat-label">通过测试</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value ${summary.failedTests > 0 ? 'status-error' : 'status-success'}">${summary.failedTests}</div>
                        <div class="stat-label">失败测试</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-value">${BenchmarkUtils.formatTime(summary.totalExecutionTime)}</div>
                        <div class="stat-label">总执行时间</div>
                    </div>
                </div>

                <div class="progress-bar">
                    <div class="progress-fill" style="width: ${(summary.passedTests / summary.totalTests) * 100}%"></div>
                </div>
                <div style="text-align: center; margin-top: 10px; color: #6c757d;">
                    成功率: ${((summary.passedTests / summary.totalTests) * 100).toFixed(1)}%
                </div>
            </div>

            <!-- 性能图表 -->
            <div class="section">
                <h2>📊 性能图表</h2>
                <div class="chart-container">
                    <canvas id="performanceChart"></canvas>
                </div>
            </div>

            <!-- 详细结果 -->
            <div class="section">
                <h2>📋 详细测试结果</h2>
                ${Array.from(groupedResults)
                  .map(
                    ([suiteName, suiteResults]) => `
                    <div class="suite-header">📦 ${suiteName}</div>
                    <table class="table">
                        <thead>
                            <tr>
                                <th>状态</th>
                                <th>测试名称</th>
                                <th>执行时间</th>
                                <th>操作/秒</th>
                                <th>内存使用</th>
                                <th>数据量</th>
                            </tr>
                        </thead>
                        <tbody>
                            ${suiteResults
                              .map(
                                (result) => `
                                <tr>
                                    <td>${result.metrics?.error ? '<span class="status-error">❌</span>' : '<span class="status-success">✅</span>'}</td>
                                    <td>${result.name}</td>
                                    <td>${result.metrics?.error ? 'ERROR' : BenchmarkUtils.formatTime(result.executionTime)}</td>
                                    <td>${result.metrics?.error ? '-' : BenchmarkUtils.formatNumber(result.operationsPerSecond)}</td>
                                    <td>${result.metrics?.error ? '-' : BenchmarkUtils.formatBytes(result.memoryUsage)}</td>
                                    <td>${BenchmarkUtils.formatNumber(result.dataSize)}</td>
                                </tr>
                            `,
                              )
                              .join('')}
                        </tbody>
                    </table>
                `,
                  )
                  .join('')}
            </div>

            <!-- 性能回归 -->
            ${
              regressions && regressions.length > 0
                ? `
            <div class="section">
                <h2>⚠️ 性能回归检测</h2>
                ${regressions
                  .filter((r) => !r.passed)
                  .map(
                    (regression) => `
                    <div class="regression-item">
                        <strong>${regression.testName}</strong> (${regression.metric})
                        <br>
                        变化: ${regression.changePercent > 0 ? '+' : ''}${regression.changePercent.toFixed(2)}%
                        (当前: ${regression.currentValue.toFixed(2)}, 基线: ${regression.baselineValue.toFixed(2)})
                        ${regression.details ? `<br><small>${regression.details}</small>` : ''}
                    </div>
                `,
                  )
                  .join('')}
            </div>
            `
                : ''
            }
        </div>
    </div>

    <script>
        // 性能图表
        const ctx = document.getElementById('performanceChart').getContext('2d');
        const chartData = {
            labels: [${results
              .filter((r) => !r.metrics?.error)
              .map((r) => `'${r.name}'`)
              .join(', ')}],
            datasets: [{
                label: '执行时间 (ms)',
                data: [${results
                  .filter((r) => !r.metrics?.error)
                  .map((r) => r.executionTime.toFixed(2))
                  .join(', ')}],
                backgroundColor: 'rgba(102, 126, 234, 0.6)',
                borderColor: 'rgba(102, 126, 234, 1)',
                borderWidth: 2,
                fill: false
            }]
        };

        new Chart(ctx, {
            type: 'bar',
            data: chartData,
            options: {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    y: {
                        beginAtZero: true,
                        title: {
                            display: true,
                            text: '执行时间 (毫秒)'
                        }
                    }
                },
                plugins: {
                    legend: {
                        display: false
                    },
                    title: {
                        display: true,
                        text: '测试执行时间对比'
                    }
                }
            }
        });
    </script>
</body>
</html>`;
  }

  /**
   * 生成CSV报告
   */
  generateCSVReport(report: BenchmarkReport): string {
    const { results } = report;
    const headers = [
      '测试名称',
      '描述',
      '执行时间(ms)',
      '内存使用(bytes)',
      '操作数',
      '操作/秒',
      '平均延迟(ms)',
      '最小延迟(ms)',
      '最大延迟(ms)',
      'P95延迟(ms)',
      'P99延迟(ms)',
      '数据量',
      '状态',
    ];

    const rows = [headers];

    for (const result of results) {
      const row = [
        result.name,
        result.description,
        result.executionTime.toFixed(2),
        result.memoryUsage.toString(),
        result.operations.toString(),
        result.operationsPerSecond.toFixed(2),
        result.averageLatency.toFixed(2),
        result.minLatency.toFixed(2),
        result.maxLatency.toFixed(2),
        result.p95Latency.toFixed(2),
        result.p99Latency.toFixed(2),
        result.dataSize.toString(),
        result.metrics?.error ? 'FAILED' : 'PASSED',
      ];
      rows.push(row);
    }

    return rows.map((row) => row.map((cell) => `"${cell}"`).join(',')).join('\n');
  }

  /**
   * 按套件分组结果
   */
  private groupResultsBySuite(results: BenchmarkResult[]): Map<string, BenchmarkResult[]> {
    const grouped = new Map<string, BenchmarkResult[]>();

    for (const result of results) {
      // 简单的套件名称推断
      let suiteName = 'Unknown';

      if (result.name.includes('三元组') || result.name.includes('链式')) {
        suiteName = 'SynapseDB Core';
      } else if (result.name.includes('文档') || result.name.includes('搜索')) {
        suiteName = 'Full-Text Search';
      } else if (
        result.name.includes('PageRank') ||
        result.name.includes('Dijkstra') ||
        result.name.includes('社区')
      ) {
        suiteName = 'Graph Algorithms';
      } else if (result.name.includes('距离') || result.name.includes('边界')) {
        suiteName = 'Spatial Geometry';
      }

      if (!grouped.has(suiteName)) {
        grouped.set(suiteName, []);
      }
      grouped.get(suiteName)!.push(result);
    }

    return grouped;
  }

  /**
   * 截断字符串
   */
  private truncateString(str: string, maxLength: number): string {
    if (str.length <= maxLength) return str;
    return str.substring(0, maxLength - 3) + '...';
  }

  /**
   * 生成性能建议
   */
  private generatePerformanceSuggestions(results: BenchmarkResult[]): string[] {
    const suggestions: string[] = [];

    // 检查慢速测试
    const slowTests = results
      .filter((r) => !r.metrics?.error && r.executionTime > 5000)
      .sort((a, b) => b.executionTime - a.executionTime);

    if (slowTests.length > 0) {
      suggestions.push(
        `发现 ${slowTests.length} 个执行时间超过5秒的测试，建议优化: ${slowTests
          .slice(0, 3)
          .map((t) => t.name)
          .join(', ')}`,
      );
    }

    // 检查内存使用
    const highMemoryTests = results
      .filter((r) => !r.metrics?.error && r.memoryUsage > 50 * 1024 * 1024) // 50MB
      .sort((a, b) => b.memoryUsage - a.memoryUsage);

    if (highMemoryTests.length > 0) {
      suggestions.push(`发现 ${highMemoryTests.length} 个高内存使用测试，建议优化内存管理`);
    }

    // 检查低吞吐量
    const lowThroughputTests = results
      .filter((r) => !r.metrics?.error && r.operationsPerSecond < 100)
      .sort((a, b) => a.operationsPerSecond - b.operationsPerSecond);

    if (lowThroughputTests.length > 0) {
      suggestions.push(`发现 ${lowThroughputTests.length} 个低吞吐量测试，建议优化算法或数据结构`);
    }

    // 检查失败测试
    const failedTests = results.filter((r) => r.metrics?.error);
    if (failedTests.length > 0) {
      suggestions.push(
        `有 ${failedTests.length} 个测试失败，需要修复: ${failedTests.map((t) => t.name).join(', ')}`,
      );
    }

    return suggestions;
  }
}

/**
 * 报告格式化工具
 */
export class ReportFormatter {
  /**
   * 格式化延迟统计
   */
  static formatLatencyStats(result: BenchmarkResult): string {
    return [
      `平均: ${result.averageLatency.toFixed(2)}ms`,
      `最小: ${result.minLatency.toFixed(2)}ms`,
      `最大: ${result.maxLatency.toFixed(2)}ms`,
      `P95: ${result.p95Latency.toFixed(2)}ms`,
      `P99: ${result.p99Latency.toFixed(2)}ms`,
    ].join(', ');
  }

  /**
   * 格式化性能指标
   */
  static formatPerformanceMetrics(result: BenchmarkResult): Record<string, string> {
    return {
      执行时间: BenchmarkUtils.formatTime(result.executionTime),
      内存使用: BenchmarkUtils.formatBytes(result.memoryUsage),
      操作数量: BenchmarkUtils.formatNumber(result.operations),
      吞吐量: `${BenchmarkUtils.formatNumber(result.operationsPerSecond)} ops/sec`,
      平均延迟: `${result.averageLatency.toFixed(2)}ms`,
      数据量: BenchmarkUtils.formatNumber(result.dataSize),
    };
  }

  /**
   * 创建性能对比表
   */
  static createComparisonTable(results: BenchmarkResult[]): string {
    const headers = ['测试名称', '执行时间', '吞吐量', '内存使用'];
    const rows = results.map((result) => [
      result.name,
      BenchmarkUtils.formatTime(result.executionTime),
      `${BenchmarkUtils.formatNumber(result.operationsPerSecond)} ops/sec`,
      BenchmarkUtils.formatBytes(result.memoryUsage),
    ]);

    // 计算列宽
    const columnWidths = headers.map((header, i) =>
      Math.max(header.length, ...rows.map((row) => row[i].length)),
    );

    // 构建表格
    const lines: string[] = [];

    // 表头
    lines.push('┌' + columnWidths.map((w) => '─'.repeat(w + 2)).join('┬') + '┐');
    lines.push('│ ' + headers.map((h, i) => h.padEnd(columnWidths[i])).join(' │ ') + ' │');
    lines.push('├' + columnWidths.map((w) => '─'.repeat(w + 2)).join('┼') + '┤');

    // 数据行
    for (const row of rows) {
      lines.push('│ ' + row.map((cell, i) => cell.padEnd(columnWidths[i])).join(' │ ') + ' │');
    }

    lines.push('└' + columnWidths.map((w) => '─'.repeat(w + 2)).join('┴') + '┘');

    return lines.join('\n');
  }

  /**
   * 生成性能趋势图（ASCII）
   */
  static generateTrendChart(
    results: BenchmarkResult[],
    metric: 'executionTime' | 'operationsPerSecond' | 'memoryUsage',
  ): string {
    if (results.length === 0) return '';

    const values = results.map((r) => r[metric]);
    const minValue = Math.min(...values);
    const maxValue = Math.max(...values);
    const range = maxValue - minValue;

    if (range === 0) return '所有值相同，无趋势图';

    const height = 10;
    const width = Math.min(results.length * 3, 60);

    const lines: string[] = [];

    for (let y = height - 1; y >= 0; y--) {
      let line = '';
      const threshold = minValue + (range * y) / (height - 1);

      for (let x = 0; x < results.length; x++) {
        const value = values[x];
        line += value >= threshold ? '█' : ' ';
        line += '  '; // 间距
      }

      lines.push(line);
    }

    // 添加标签
    const labels = results.map((r) => r.name.substring(0, 8));
    lines.push('-'.repeat(width));
    lines.push(labels.join('  '));

    return lines.join('\n');
  }
}
