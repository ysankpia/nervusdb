import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promises as fs } from 'node:fs';

import { SynapseDB } from '@/synapseDb';
import { readPagedManifest, pageFileName } from '@/storage/pagedIndex';
import { checkStrict } from '@/maintenance/check';
import { repairCorruptedOrders } from '@/maintenance/repair';

describe('按序修复损坏页（CRC）', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-repair-'));
    dbPath = join(workspace, 'repair.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('损坏某页后 strict 检查失败，执行按序修复后恢复正常', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 4 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();

    // 人为破坏 SPO 页文件中的第一个页块的一个字节
    const manifest = await readPagedManifest(`${dbPath}.pages`);
    const lookup = manifest!.lookups.find((l) => l.order === 'SPO')!;
    const first = lookup.pages[0];
    const file = join(`${dbPath}.pages`, pageFileName('SPO'));
    const fd = await fs.open(file, 'r+');
    try {
      const buf = Buffer.allocUnsafe(first.length);
      await fd.read(buf, 0, first.length, first.offset);
      buf[0] = (buf[0] ^ 0x01) & 0xff; // 翻转首字节
      await fd.write(buf, 0, first.length, first.offset);
    } finally {
      await fd.close();
    }

    const bad = await checkStrict(dbPath);
    expect(bad.ok).toBe(false);
    expect(bad.errors.some((e) => e.order === 'SPO')).toBe(true);

    const repaired = await repairCorruptedOrders(dbPath);
    expect(repaired.repairedOrders).toContain('SPO');

    const ok = await checkStrict(dbPath);
    expect(ok.ok).toBe(true);

    // 重新打开数据库以加载新的 manifest
    const db2 = await SynapseDB.open(dbPath);
    const results = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(3);
  });
});
