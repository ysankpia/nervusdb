/**
 * NervusDB 顶层 CLI 分发器
 *
 * 用法：
 *   nervusdb <command> [...args]
 *
 * 可用命令：
 *   bench <db> [count]
 *   cypher <db> [--query|-q <cypher>] [--file <path>] [--readonly] [--params JSON] [--format table|json] [--limit N]
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
    'NervusDB CLI',
    '',
    '用法:',
    '  nervusdb <command> [...args]',
    '',
    '命令:',
    '  bench <db> [count]',
    '  cypher <db> [--query|-q <cypher>] [--file <path>] [--readonly] [--params JSON] [--format table|json] [--limit N]',
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
    case 'bench': {
      const code = await run(rel('./bench.js'), rest);
      process.exit(code);
      break;
    }
    case 'cypher': {
      const code = await run(rel('./cypher.js'), ['cypher', ...rest]);
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
