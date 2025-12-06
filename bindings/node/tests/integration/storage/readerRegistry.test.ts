import { describe, it, expect, beforeEach } from 'vitest';
import { mkdtempSync, rmSync, readdirSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  addReader,
  getActiveReaders,
  removeReader,
  cleanupStaleReaders,
  getActiveEpochs,
  isEpochInUse,
} from '@/core/storage/readerRegistry';

describe('ReaderRegistry · 读者登记', () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'synapsedb-readers-'));
  });

  it('新增/查询/删除读者文件', async () => {
    const pid = process.pid;
    const now = Date.now();
    await addReader(dir, { pid, epoch: 1, ts: now });

    const readers = await getActiveReaders(dir);
    expect(readers.length).toBe(1);
    expect(readers[0].pid).toBe(pid);
    expect(readers[0].epoch).toBe(1);

    expect(await isEpochInUse(dir, 1)).toBe(true);
    expect(await isEpochInUse(dir, 2)).toBe(false);
    expect(await getActiveEpochs(dir)).toEqual([1]);

    await removeReader(dir, pid);
    const after = await getActiveReaders(dir);
    expect(after.length).toBe(0);
  });

  it('清理过期读者文件', async () => {
    const pid = process.pid;
    const ts = Date.now();
    await addReader(dir, { pid, epoch: 2, ts });

    // 人工将 mtime 调整到很早，确保被视为过期
    const readersDir = join(dir, 'readers');
    const files = readdirSync(readersDir).filter((f) => f.endsWith('.reader'));
    for (const f of files) {
      const p = join(readersDir, f);
      const old = new Date(Date.now() - 60_000);
      utimesSync(p, old, old);
    }

    await cleanupStaleReaders(dir, 1000); // 1s 阈值
    const readers = await getActiveReaders(dir);
    expect(readers.length).toBe(0);
  });

  // 清理临时目录
  afterEach(() => {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {}
  });
});
