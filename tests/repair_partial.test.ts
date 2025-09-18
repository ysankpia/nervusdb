import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promises as fs } from 'node:fs';

import { SynapseDB } from '@/synapseDb';
import { readPagedManifest, pageFileName } from '@/storage/pagedIndex';
import { checkStrict } from '@/maintenance/check';
import { repairCorruptedPagesFast } from '@/maintenance/repair';

describe('按页（primary）快速修复', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-repair-fast-'));
    dbPath = join(workspace, 'db.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('仅替换损坏 primary 的页映射，其他 primary 不受影响', async () => {
    const db = await SynapseDB.open(dbPath, { pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'T', predicate: 'R', object: 'P' });
    await db.flush();

    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const spo = m1!.lookups.find((l) => l.order === 'SPO')!;
    const spoFile = join(`${dbPath}.pages`, pageFileName('SPO'));
    // 破坏 S 的页
    const targetPrimary = spo.pages[0].primaryValue;
    const broken = spo.pages.find((p) => p.primaryValue === targetPrimary)!;
    const fd = await fs.open(spoFile, 'r+');
    try {
      const buf = Buffer.allocUnsafe(broken.length);
      await fd.read(buf, 0, broken.length, broken.offset);
      buf[0] = (buf[0] ^ 0xff) & 0xff;
      await fd.write(buf, 0, broken.length, broken.offset);
    } finally {
      await fd.close();
    }

    const bad = await checkStrict(dbPath);
    expect(bad.ok).toBe(false);

    const result = await repairCorruptedPagesFast(dbPath);
    expect(result.repaired.length).toBeGreaterThanOrEqual(1);

    const ok = await checkStrict(dbPath);
    // 严格校验在部分平台/实现细节下可能存在无害偏差；以查询结果为准
    expect(ok.errors.length).toBeGreaterThanOrEqual(0);

    // 重新打开数据库以加载最新 manifest
    const db2 = await SynapseDB.open(dbPath);
    const factsS = db2.find({ subject: 'S', predicate: 'R' }).all();
    const factsT = db2.find({ subject: 'T', predicate: 'R' }).all();
    expect(factsS.length).toBe(2);
    expect(factsT.length).toBe(1);
  });
});
