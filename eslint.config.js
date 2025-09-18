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
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-return': 'off'
    }
  },
  prettierConfig
);
