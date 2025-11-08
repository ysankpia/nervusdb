#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';

const EXCLUDE_PATTERN = "tests/**/*performance*.test.ts";
const args = process.argv.slice(2);
const listOnly = args.includes('--list');
const explicitTargets = args.filter((arg) => !arg.startsWith('--'));

const targets = explicitTargets.length > 0 ? explicitTargets : collectDefaultTargets();

if (!targets.length) {
  console.error('No test targets were detected. Ensure tests/unit or tests/integration exist.');
  process.exit(1);
}

if (listOnly) {
  targets.forEach((target) => console.log(target));
  process.exit(0);
}

const env = {
  ...process.env,
  NODE_OPTIONS: process.env.NODE_OPTIONS ?? '--max-old-space-size=8192',
};

for (const target of targets) {
  const fsPath = path.normalize(target);
  if (!existsSync(fsPath)) {
    console.log(`ℹ️  Skipping ${target} (missing)`);
    continue;
  }

  console.log(`\n▶️  Vitest: ${target}`);
  const result = spawnSync(
    'pnpm',
    ['exec', 'vitest', 'run', '--reporter=dot', '--exclude', EXCLUDE_PATTERN, target],
    { stdio: 'inherit', env }
  );

  if (result.status !== 0) {
    console.error(`❌ Vitest failed for ${target}`);
    process.exit(result.status ?? 1);
  }
}

function collectDefaultTargets() {
  const targets = [];

  const unitRoot = 'tests/unit';
  if (existsSync(unitRoot)) {
    const entries = readdirSync(unitRoot, { withFileTypes: true });
    const dirs = [];
    const files = [];

    for (const entry of entries) {
      if (entry.name.startsWith('.')) continue;
      const relPath = path.posix.join('tests/unit', entry.name);
      if (entry.isDirectory()) {
        dirs.push(relPath);
      } else if (entry.isFile() && /\.test\.[cm]?[tj]sx?$/.test(entry.name)) {
        files.push(relPath);
      }
    }

    dirs.sort((a, b) => a.localeCompare(b));
    files.sort((a, b) => a.localeCompare(b));
    targets.push(...dirs, ...files);
  }

  for (const extra of ['tests/integration', 'tests/system']) {
    if (existsSync(extra)) {
      targets.push(extra);
    }
  }

  return targets;
}
