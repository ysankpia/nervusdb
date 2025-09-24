import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const rootDir = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    testTimeout: 20000,
    include: ['tests/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      reportsDirectory: resolve(rootDir, 'coverage'),
      reporter: ['text', 'lcov'],
      include: ['src/**/*.ts'],
      // 说明：以下排除项不计入当前覆盖率门槛
      // - src/cli/**: CLI 封装，覆盖率独立评估
      // - **/*.d.ts/**.config.*: 类型与配置文件
      // - src/types/**: 仅类型增强文件，不生成可执行代码
      // - src/spatial/**: 空间计算与 R-Tree 模块，后续将单独补充专项测试再纳入门槛
      exclude: [
        'src/cli/**',
        '**/*.d.ts',
        '**/*.config.*',
        'cspell.config.cjs',
        'src/types/**',
        'src/spatial/**',
        'src/fulltext/**',
        'src/benchmark/**',
        // 暂未纳入门槛的基础设施/占位文件
        'src/graph/paths.ts',
        'src/query/iterator.ts',
        'src/query/gremlin/step.ts',
        'src/query/path/bidirectional.ts',
        'src/query/pattern/ast.ts'
      ],
      thresholds: {
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
