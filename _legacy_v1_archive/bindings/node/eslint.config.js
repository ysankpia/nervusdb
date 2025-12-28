import js from '@eslint/js';
import prettierConfig from 'eslint-config-prettier';
import prettierPlugin from 'eslint-plugin-prettier';
import tseslint from 'typescript-eslint';

const commonTypeScriptRules = {
  '@typescript-eslint/explicit-function-return-type': 'off',
  '@typescript-eslint/no-misused-promises': [
    'error',
    {
      checksVoidReturn: false
    }
  ],
  // 允许空的 catch 块以容忍脚本中的“最佳努力清理”写法
  'no-empty': ['error', { allowEmptyCatch: true }],
  'prettier/prettier': [
    'error',
    {
      singleQuote: true,
      trailingComma: 'all',
      semi: true
    }
  ]
};

export default tseslint.config(
  {
    ignores: ['dist/**', 'coverage/**', 'node_modules/**']
  },
  js.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    files: ['src/**/*.ts', 'tests/**/*.ts'],
    plugins: {
      prettier: prettierPlugin
    },
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.json', './tsconfig.vitest.json'],
        tsconfigRootDir: import.meta.dirname
      }
    },
    rules: commonTypeScriptRules
  },
  {
    files: ['tests/**/*.ts'],
    rules: {
      // 测试场景放宽类型与安全性约束，便于编写断言与桩实现
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-non-null-asserted-optional-chain': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      // 测试用例允许便捷地使用 async 回调（即使暂未 await）
      '@typescript-eslint/require-await': 'off',
      // 解析器/字符串断言中可能出现的转义写法不做强制
      'no-useless-escape': 'off'
    }
  },
  {
    files: ['src/cli/**/*.ts', 'src/maintenance/**/*.ts'],
    rules: {
      // CLI/维护脚本偏工程化，放宽 any 与 unsafe 访问限制
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      // 容忍少量未使用变量（如预留参数/调试片段）
      '@typescript-eslint/no-unused-vars': 'warn'
    }
  },
  {
    files: [
      'src/query/graphql/**/*.ts',
      'src/query/gremlin/**/*.ts',
      'src/query/pattern/**/*.ts',
      'src/extensions/query/graphql/**/*.ts',
      'src/extensions/query/gremlin/**/*.ts',
      'src/extensions/query/pattern/**/*.ts'
    ],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/require-await': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/no-unnecessary-type-assertion': 'off',
      '@typescript-eslint/no-base-to-string': 'off',
      '@typescript-eslint/restrict-template-expressions': 'off',
      '@typescript-eslint/no-redundant-type-constituents': 'off',
      'prettier/prettier': 'off',
      'prefer-const': 'off'
    }
  },
  {
    files: ['src/query/**/*.ts', 'src/core/query/**/*.ts', 'src/extensions/query/**/*.ts', 'src/native/**/*.ts'],
    rules: {
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      'no-case-declarations': 'off',
      'prefer-const': 'off',
      '@typescript-eslint/no-require-imports': 'off'
    }
  },
  prettierConfig
);
