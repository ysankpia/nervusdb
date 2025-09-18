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
  const lines = [
    'SynapseDB CLI',
    '',
    '用法:',
    '  synapsedb <command> [...args]',
    '',
    '命令:',
    '  check <db> [--summary|--strict]',
    '  repair <db> [--fast]',
    '  compact [...args]',
    '  stats <db> [--txids[=N]] [--txids-window=MIN]',
    '  txids <db> [--list[=N]] [--since=MIN] [--session=ID] [--max=N] [--clear]',
    '  dump <db> [...args]',
    '  bench <db> [count] [mode]',
    '  auto-compact [...args]',
    '  gc <db> [...args]',
    '  hot <db> [...args]',
    '  repair-page <db> <order> <primary>',
  ];
  console.log(lines.join('\n'));
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
      break;
    }
    case 'compact': {
      const code = await run(rel('./compact.js'), rest);
      process.exit(code);
      break;
    }
    case 'stats': {
      const code = await run(rel('./stats.js'), rest);
      process.exit(code);
      break;
    }
    case 'txids': {
      const code = await run(rel('./txids.js'), rest);
      process.exit(code);
      break;
    }
    case 'dump': {
      const code = await run(rel('./dump.js'), rest);
      process.exit(code);
      break;
    }
    case 'bench': {
      const code = await run(rel('./bench.js'), rest);
      process.exit(code);
      break;
    }
    case 'auto-compact': {
      const code = await run(rel('./auto_compact.js'), rest);
      process.exit(code);
      break;
    }
    case 'gc': {
      const code = await run(rel('./gc.js'), rest);
      process.exit(code);
      break;
    }
    case 'hot': {
      const code = await run(rel('./hot.js'), rest);
      process.exit(code);
      break;
    }
    case 'repair-page': {
      const code = await run(rel('./repair_page.js'), rest);
      process.exit(code);
      break;
    }
    default:
      console.error(`未知命令: ${cmd}`);
      usage();
      process.exit(1);
  }
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
