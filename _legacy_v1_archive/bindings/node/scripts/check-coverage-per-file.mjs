#!/usr/bin/env node
// 粗暴且实用的每文件覆盖率门槛校验脚本
// 读取 coverage/coverage-summary.json，筛选 src/** 下的文件
// 排除：src/cli/**、src/benchmark/**、src/examples/**
// 对每个文件要求 lines.pct >= 75

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const MIN_PCT = Number(process.env.COVERAGE_FILE_MIN || 75);
const summaryPath = resolve('coverage/coverage-summary.json');

let summary;
try {
  summary = JSON.parse(readFileSync(summaryPath, 'utf8'));
} catch (e) {
  console.error(`无法读取覆盖率摘要: ${summaryPath}`);
  console.error(String(e));
  process.exit(2);
}

const isExcluded = (file) =>
  file.startsWith('src/cli/') || file.startsWith('src/benchmark/') || file.startsWith('src/examples/');

const failures = [];
for (const [file, data] of Object.entries(summary)) {
  if (!file.startsWith('src/')) continue;
  if (isExcluded(file)) continue;
  const pct = data?.total?.lines?.pct ?? 0;
  if (pct < MIN_PCT) {
    failures.push({ file, pct });
  }
}

if (failures.length > 0) {
  console.error(`每文件覆盖率未达标(>=${MIN_PCT}%):`);
  for (const f of failures.sort((a, b) => a.pct - b.pct)) {
    console.error(`  ${f.pct.toFixed(2).padStart(6)}%  ${f.file}`);
  }
  process.exit(1);
} else {
  console.log(`✅ 每文件覆盖率均满足 >= ${MIN_PCT}%`);
}

