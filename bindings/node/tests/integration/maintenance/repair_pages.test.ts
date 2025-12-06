import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promises as fs } from 'node:fs';

import { NervusDB } from '@/synapseDb';
import { readPagedManifest, pageFileName } from '@/core/storage/pagedIndex';
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
    // 强制清理readers目录中的所有文件，确保完全清理
    try {
      const readersDir = join(dbPath + '.pages', 'readers');
      // 重试清理逻辑，处理可能的竞态条件
      for (let attempt = 0; attempt < 5; attempt++) {
        try {
          const files = await readdir(readersDir);
          for (const file of files) {
            try {
              await unlink(join(readersDir, file));
            } catch {
              // 忽略删除失败
            }
          }
          await rmdir(readersDir);
          break; // 成功清理，退出重试循环
        } catch (err: any) {
          if (err?.code === 'ENOTEMPTY' && attempt < 4) {
            // 目录不为空，等待一下再重试
            await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
            continue;
          }
          // 其他错误或最后一次尝试失败，忽略
          break;
        }
      }
    } catch {
      // 忽略所有清理错误
    }

    await rm(workspace, { recursive: true, force: true });
  });

  it('损坏某页后 strict 检查失败，执行按序修复后恢复正常', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 4 });
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
    const db2 = await NervusDB.open(dbPath);
    const results = db2.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(3);
  });
});
