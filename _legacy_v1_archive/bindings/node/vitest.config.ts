import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const rootDir = dirname(fileURLToPath(import.meta.url));
const disableCoverageThresholds = process.env.VITEST_DISABLE_THRESHOLDS === '1';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    testTimeout: 20000,
    // IO 与磁盘操作较多，使用 forks 池进行适度并发，避免内存累积
    pool: 'forks',
    poolOptions: {
      forks: {
        // 允许多进程并行运行，避免单进程内存累积问题
        minForks: 1,
        maxForks: 2, // 减少并发度，降低内存压力
        execArgv: ['--max-old-space-size=8192'] // 增加每个fork进程的内存(应对JIT编译峰值)
      }
    },
    // 禁用文件级并发，确保任何时候只有一个测试在初始化，避免内存峰值
    sequence: {
      concurrent: false
    },
    include: ['tests/**/*.test.ts'],
    exclude: ['**/node_modules/**', '**/dist/**', '**/wasm*.test.ts'],
    globalSetup: resolve(rootDir, 'tests/setup/global-cleanup.ts'),
    coverage: {
      provider: 'v8',
      reportsDirectory: resolve(rootDir, 'coverage'),
      // 增加 json-summary 以便后续按文件阈值检查脚本解析
      reporter: ['text', 'lcov', 'json-summary'],
      include: ['src/**/*.ts'],
      // 说明：以下排除项不计入当前覆盖率门槛
      // - src/cli/**: CLI 封装，覆盖率独立评估
      // - src/benchmark/**: 基准测试框架（将迁移到 benchmarks/*.mjs），不影响产品质量指标
      // - src/examples/**: 示例代码，不计入质量门槛
      // - **/*.d.ts/**.config.*: 类型与配置文件
      exclude: [
        'src/cli/**',
        'src/benchmark/**',
        'src/examples/**',
        '**/*.d.ts',
        '**/*.config.*',
        'cspell.config.cjs'
      ],
      thresholds: disableCoverageThresholds
        ? undefined
        : {
            statements: 80,
            branches: 75,
            functions: 80,
            lines: 80
          }
    }
  },
  resolve: {
    alias: {
      '@': resolve(rootDir, 'src')
    }
  }
});
