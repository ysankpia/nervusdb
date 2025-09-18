/** lint-staged v15+ 显式配置（CommonJS） */
module.exports = {
  '*.{ts,tsx}': ['pnpm exec eslint --fix --max-warnings=0', 'pnpm exec cspell lint --no-progress --no-summary'],
  '*.{md,json,yml,yaml}': ['pnpm exec prettier --write', 'pnpm exec cspell lint --no-progress --no-summary']
};
