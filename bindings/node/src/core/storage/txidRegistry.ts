import { promises as fs } from 'node:fs';
import { dirname, join } from 'node:path';

export interface TxIdEntry {
  id: string;
  ts: number;
  sessionId?: string;
}

export interface TxIdRegistryData {
  version: number;
  txIds: TxIdEntry[];
  max?: number;
}

const FILE = 'txids.json';

export async function readTxIdRegistry(directory: string): Promise<TxIdRegistryData> {
  const file = join(directory, FILE);
  try {
    const buf = await fs.readFile(file);
    return JSON.parse(buf.toString('utf8')) as TxIdRegistryData;
  } catch {
    return { version: 1, txIds: [] } as TxIdRegistryData;
  }
}

export async function writeTxIdRegistry(directory: string, data: TxIdRegistryData): Promise<void> {
  const file = join(directory, FILE);
  const tmp = `${file}.tmp`;
  const json = Buffer.from(JSON.stringify(data, null, 2), 'utf8');
  const fh = await fs.open(tmp, 'w');
  try {
    await fh.write(json, 0, json.length, 0);
    await fh.sync();
  } finally {
    await fh.close();
  }
  await fs.rename(tmp, file);
  try {
    const dh = await fs.open(dirname(file), 'r');
    try {
      await dh.sync();
    } finally {
      await dh.close();
    }
  } catch {
    // ignore
  }
}

export function toSet(reg: TxIdRegistryData): Set<string> {
  return new Set(reg.txIds.map((x) => x.id));
}

export function mergeTxIds(
  reg: TxIdRegistryData,
  items: Array<{ id: string; ts?: number; sessionId?: string }>,
  max: number | undefined,
): TxIdRegistryData {
  const seen = new Set(reg.txIds.map((x) => x.id));
  const now = Date.now();
  for (const item of items) {
    const id = item.id;
    if (!id) continue;
    if (seen.has(id)) continue;
    reg.txIds.push({ id, ts: item.ts ?? now, sessionId: item.sessionId });
    seen.add(id);
  }
  // 截断到最近 max 个
  if (max && max > 0 && reg.txIds.length > max) {
    reg.txIds.sort((a, b) => b.ts - a.ts);
    reg.txIds = reg.txIds.slice(0, max);
  }
  if (max && max > 0) reg.max = max;
  return reg;
}
