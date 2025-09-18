/** lint-staged v15+ 显式配置（CommonJS） */
module.exports = {
  'src/**/*.{ts,tsx}': ['pnpm exec eslint --fix --max-warnings=0'],
  'tests/**/*.ts': ['pnpm exec eslint --fix --max-warnings=0'],
  'README.md': ['pnpm exec prettier --write'],
  'docs/**/*.md': ['pnpm exec prettier --write'],
  '.github/**/*.{yml,yaml}': ['pnpm exec prettier --write'],
  'package.json': ['pnpm exec prettier --write']
};
