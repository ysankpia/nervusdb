#!/usr/bin/env node
/**
 * SynapseDB 顶层 CLI 分发器
 *
 * 用法：
 *   synapsedb <command> [...args]
 *
 * 可用命令：
 *   check <db> [--summary|--strict]
 *   repair <db> [--fast]
 *   compact [...args]
 *   stats <db> [--txids[=N]] [--txids-window=MINUTES]
 *   txids <db> [--list[=N]] [--since=MINUTES] [--session=ID] [--max=N] [--clear]
 *   dump <db> [...args]
 *   bench <db> [count] [mode]
 *   auto-compact [...args]
 *   gc <db> [...args]
 *   hot <db> [...args]
 *   repair-page <db> <order> <primary>
 */

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));

function run(file: string, args: string[]): Promise<number> {
  return new Promise((resolveCode) => {
    const node = process.execPath;
    const child = spawn(node, [file, ...args], { stdio: 'inherit' });
    child.on('exit', (code) => resolveCode(code ?? 0));
  });
}

function usage(): void {
  console.log(`SynapseDB CLI\n\n用法:\n  synapsedb <command> [...args]\n\n命令:\n  check <db> [--summary|--strict]\n  repair <db> [--fast]\n  compact [...args]\n  stats <db> [--txids[=N]] [--txids-window=MIN]\n  txids <db> [--list[=N]] [--since=MIN] [--session=ID] [--max=N] [--clear]\n  dump <db> [...args]\n  bench <db> [count] [mode]\n  auto-compact [...args]\n  gc <db> [...args]\n  hot <db> [...args]\n  repair-page <db> <order> <primary>\n`);
}

async function main() {
  const [cmd, ...rest] = process.argv.slice(2);
  if (!cmd || cmd === '-h' || cmd === '--help') {
    usage();
    process.exit(0);
  }

  const rel = (p: string) => resolve(here, p);
  switch (cmd) {
    case 'check':
    case 'repair': {
      const code = await run(rel('./check.js'), [cmd, ...rest]);
      process.exit(code);
    }
    case 'compact': {
      const code = await run(rel('./compact.js'), rest);
      process.exit(code);
    }
    case 'stats': {
      const code = await run(rel('./stats.js'), rest);
      process.exit(code);
    }
    case 'txids': {
      const code = await run(rel('./txids.js'), rest);
      process.exit(code);
    }
    case 'dump': {
      const code = await run(rel('./dump.js'), rest);
      process.exit(code);
    }
    case 'bench': {
      const code = await run(rel('./bench.js'), rest);
      process.exit(code);
    }
    case 'auto-compact': {
      const code = await run(rel('./auto_compact.js'), rest);
      process.exit(code);
    }
    case 'gc': {
      const code = await run(rel('./gc.js'), rest);
      process.exit(code);
    }
    case 'hot': {
      const code = await run(rel('./hot.js'), rest);
      process.exit(code);
    }
    case 'repair-page': {
      const code = await run(rel('./repair_page.js'), rest);
      process.exit(code);
    }
    default:
      console.error(`未知命令: ${cmd}`);
      usage();
      process.exit(1);
  }
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();

