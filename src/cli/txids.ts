import { join } from 'node:path';

async function main() {
  const args = process.argv.slice(2);
  const dbPath = args[0];
  if (!dbPath) {
    console.log('用法: pnpm db:txids <db> [--list] [--max=N] [--clear]');
    process.exit(1);
  }

  const dir = `${dbPath}.pages`;
  const { readTxIdRegistry, writeTxIdRegistry } = await import('../storage/txidRegistry');

  const maxArg = args.find((a) => a.startsWith('--max='));
  const setMax = maxArg ? Number(maxArg.split('=')[1]) : undefined;
  const list = args.includes('--list') || (!args.includes('--clear') && !setMax);
  const sinceArg = args.find((a) => a.startsWith('--since='));
  const sinceMin = sinceArg ? Number(sinceArg.split('=')[1]) : undefined;
  const sessionArg = args.find((a) => a.startsWith('--session='));
  const sessionFilter = sessionArg ? String(sessionArg.split('=')[1]) : undefined;
  const clear = args.includes('--clear');

  const reg = await readTxIdRegistry(dir);
  if (clear) {
    reg.txIds = [];
    await writeTxIdRegistry(dir, reg);
    console.log('txId 注册表已清空');
    return;
  }

  if (setMax && setMax > 0) {
    reg.txIds.sort((a, b) => b.ts - a.ts);
    if (reg.txIds.length > setMax) reg.txIds = reg.txIds.slice(0, setMax);
    reg.max = setMax;
    await writeTxIdRegistry(dir, reg);
    console.log(`容量上限已设置为 ${setMax}，当前条目 ${reg.txIds.length}`);
    return;
  }

  if (list) {
    const nArg = args.find((a) => a.startsWith('--list='));
    const limit = nArg ? Number(nArg.split('=')[1]) : 50;
    let items = [...reg.txIds].sort((a, b) => b.ts - a.ts);
    if (sinceMin && sinceMin > 0) {
      const since = Date.now() - sinceMin * 60_000;
      items = items.filter((x) => x.ts >= since);
    }
    if (sessionFilter) {
      items = items.filter((x) => (x.sessionId ?? 'unknown') === sessionFilter);
    }
    items = items.slice(0, limit);
    console.log(
      JSON.stringify(
        {
          count: reg.txIds.length,
          max: reg.max ?? null,
          items,
        },
        null,
        2,
      ),
    );
    return;
  }
}

// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
