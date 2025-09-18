import { promises as fs } from 'node:fs';
import { join, dirname } from 'node:path';

export interface ReaderInfo {
  pid: number;
  epoch: number;
  ts: number;
}

export interface ReaderRegistry {
  version: number;
  readers: ReaderInfo[];
}

const FILE = 'readers.json';

export async function readRegistry(directory: string): Promise<ReaderRegistry> {
  const file = join(directory, FILE);
  try {
    const buf = await fs.readFile(file);
    return JSON.parse(buf.toString('utf8')) as ReaderRegistry;
  } catch {
    return { version: 1, readers: [] } as ReaderRegistry;
  }
}

async function writeRegistry(directory: string, reg: ReaderRegistry): Promise<void> {
  const file = join(directory, FILE);
  const tmp = `${file}.tmp`;
  const json = Buffer.from(JSON.stringify(reg, null, 2), 'utf8');
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
    try { await dh.sync(); } finally { await dh.close(); }
  } catch {}
}

export async function addReader(directory: string, info: ReaderInfo): Promise<void> {
  const reg = await readRegistry(directory);
  const existing = reg.readers.find((r) => r.pid === info.pid);
  if (existing) {
    existing.epoch = info.epoch;
    existing.ts = info.ts;
  } else {
    reg.readers.push(info);
  }
  await writeRegistry(directory, reg);
}

export async function removeReader(directory: string, pid: number): Promise<void> {
  const reg = await readRegistry(directory);
  reg.readers = reg.readers.filter((r) => r.pid !== pid);
  await writeRegistry(directory, reg);
}

export async function getActiveReaders(directory: string): Promise<ReaderInfo[]> {
  const reg = await readRegistry(directory);
  return reg.readers;
}

