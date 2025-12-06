import { promises as fs } from 'node:fs';
import { join, dirname } from 'node:path';

import type { IndexOrder } from './tripleIndexes.js';

export interface HotnessData {
  version: number;
  updatedAt: number;
  counts: Record<IndexOrder, Record<string, number>>; // primaryValue -> count
}

const FILE = 'hotness.json';

export async function readHotness(directory: string): Promise<HotnessData> {
  const file = join(directory, FILE);
  try {
    const buf = await fs.readFile(file);
    return JSON.parse(buf.toString('utf8')) as HotnessData;
  } catch {
    return {
      version: 1,
      updatedAt: Date.now(),
      counts: { SPO: {}, SOP: {}, POS: {}, PSO: {}, OSP: {}, OPS: {} },
    } as HotnessData;
  }
}

export async function writeHotness(directory: string, data: HotnessData): Promise<void> {
  const file = join(directory, FILE);
  const tmp = `${file}.tmp`;
  const json = Buffer.from(JSON.stringify({ ...data, updatedAt: Date.now() }, null, 2), 'utf8');
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
    // 忽略目录同步失败（跨平台容忍）
  }
}
