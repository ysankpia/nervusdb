/** lint-staged v15+ 显式配置（CommonJS） */
module.exports = {
  // 仅对核心路径执行严格 Lint（与 pnpm lint:core 一致）
  'src/{index.ts,synapseDb.ts,query/**/*.ts,storage/**/*.ts}': [
    'pnpm exec eslint --fix --max-warnings=0'
  ],
  'tests/**/*.ts': ['pnpm exec eslint --fix --max-warnings=0'],
  'README.md': ['pnpm exec prettier --write'],
  'docs/**/*.md': ['pnpm exec prettier --write'],
  '.agents/**/*.md': ['pnpm exec prettier --write'],
  '.github/**/*.{yml,yaml}': ['pnpm exec prettier --write'],
  'package.json': ['pnpm exec prettier --write']
};
