#!/usr/bin/env node
/**
 * Cypher 命令行执行器
 *
 * 用法：
 *   nervusdb cypher <db> [--query|-q <cypher>] [--file <path>] \
 *     [--readonly] [--params '{"k":"v"}'] [--format table|json] [--limit N]
 */

import { readFile } from 'node:fs/promises';
import { NervusDB } from '../nervusDb.js';

interface Args {
  dbPath: string;
  query?: string;
  file?: string;
  readonly?: boolean;
  params?: Record<string, unknown>;
  format?: 'table' | 'json';
  limit?: number;
}

function parseArgs(argv: string[]): Args {
  const args: Args = { dbPath: '' } as Args;
  const it = argv[Symbol.iterator]();
  const next = () => it.next();

  const first = next();
  if (first.done) throw new Error('缺少 <db> 参数');
  args.dbPath = first.value;

  for (let cur = next(); !cur.done; cur = next()) {
    const a = cur.value;
    if (a === '--query' || a === '-q') {
      const v = next();
      if (v.done) throw new Error('--query 需要参数');
      args.query = v.value;
    } else if (a.startsWith('--query=')) {
      args.query = a.slice('--query='.length);
    } else if (a === '--file') {
      const v = next();
      if (v.done) throw new Error('--file 需要参数');
      args.file = v.value;
    } else if (a.startsWith('--file=')) {
      args.file = a.slice('--file='.length);
    } else if (a === '--readonly') {
      args.readonly = true;
    } else if (a === '--params') {
      const v = next();
      if (v.done) throw new Error('--params 需要 JSON 参数');
      try {
        args.params = JSON.parse(v.value);
      } catch (e) {
        throw new Error(`无法解析 --params: ${(e as Error).message}`);
      }
    } else if (a.startsWith('--params=')) {
      try {
        args.params = JSON.parse(a.slice('--params='.length));
      } catch (e) {
        throw new Error(`无法解析 --params: ${(e as Error).message}`);
      }
    } else if (a === '--format') {
      const v = next();
      if (v.done) throw new Error('--format 需要参数');
      const f = v.value.toLowerCase();
      args.format = f === 'json' ? 'json' : 'table';
    } else if (a.startsWith('--format=')) {
      const f = a.slice('--format='.length).toLowerCase();
      args.format = f === 'json' ? 'json' : 'table';
    } else if (a === '--limit') {
      const v = next();
      if (v.done) throw new Error('--limit 需要数值');
      args.limit = Number(v.value);
    } else if (a.startsWith('--limit=')) {
      args.limit = Number(a.slice('--limit='.length));
    } else {
      throw new Error(`未知参数: ${a}`);
    }
  }

  return args;
}

function usage(): void {
  const lines = [
    '用法:',
    '  nervusdb cypher <db> [--query|-q <cypher>] [--file <path>]',
    '                       [--readonly] [--params JSON]',
    '                       [--format table|json] [--limit N]',
    '',
    '示例:',
    "  nervusdb cypher data.redb -q 'MATCH (n) RETURN n LIMIT 5' --readonly",
    '  nervusdb cypher data.redb --file query.cql --params \'{"minAge":25}\'',
  ];
  console.log(lines.join('\n'));
}

function formatResults(
  records: Array<Record<string, unknown>>,
  options: { limit?: number; format?: 'table' | 'json' },
): string {
  const { limit = 100, format = 'table' } = options;
  const limited = records.slice(0, limit);

  if (format === 'json') {
    return JSON.stringify(limited, null, 2);
  }

  // Simple table format
  if (limited.length === 0) {
    return '(no results)';
  }

  const keys = Object.keys(limited[0]);
  const header = keys.join('\t');
  const rows = limited.map((row) => keys.map((k) => String(row[k] ?? '')).join('\t'));

  return [header, '-'.repeat(header.length), ...rows].join('\n');
}

async function main() {
  try {
    const [cmd, ...rest] = process.argv.slice(2);
    const argv = cmd === 'cypher' ? rest : [cmd, ...rest];
    const args = parseArgs(argv);

    if (!args.query && !args.file) {
      usage();
      throw new Error('必须通过 --query 或 --file 指定 Cypher 语句');
    }

    const db = await NervusDB.open(args.dbPath, {
      experimental: { cypher: true },
    });

    const text = args.query ?? (await readFile(args.file!, 'utf8'));

    const res = args.readonly
      ? await db.cypherRead(text, args.params ?? {})
      : await db.cypherQuery(text, args.params ?? {});

    const formatted = formatResults(res.records as Array<Record<string, unknown>>, {
      limit: args.limit ?? 100,
      format: args.format ?? 'table',
    });

    console.log(formatted);
    console.log('');
    console.log(`Returned ${res.records.length} row(s)`);

    await db.close();
  } catch (error) {
    console.error(`执行失败: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
  }
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
