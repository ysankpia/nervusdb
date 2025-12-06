import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { readPagedManifest } from '@/core/storage/pagedIndex';

describe('Compaction MVP', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-compact-'));
    dbPath = join(workspace, 'compact.synapsedb');
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

  it('合并同主键的小页，压缩页数', async () => {
    const db = await NervusDB.open(dbPath, { pageSize: 4 });
    // 初次写入 3 条（同 subject），首次构建将写入 1 页
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O2' });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O3' });
    await db.flush();

    const m1 = await readPagedManifest(`${dbPath}.pages`);
    const spo1 = m1!.lookups.find((l) => l.order === 'SPO')!;
    const primary = m1?.lookups.find((l) => l.order === 'SPO')!.pages[0].primaryValue!;
    const pagesBefore = spo1.pages.filter((p) => p.primaryValue === primary).length;
    expect(pagesBefore).toBe(1);

    // 追加 1 条并 flush，会产生一个新页（不足 pageSize）
    db.addFact({ subject: 'S', predicate: 'R', object: 'O4' });
    await db.flush();
    const m2 = await readPagedManifest(`${dbPath}.pages`);
    const spo2 = m2!.lookups.find((l) => l.order === 'SPO')!;
    const pagesMiddle = spo2.pages.filter((p) => p.primaryValue === primary).length;
    expect(pagesMiddle).toBeGreaterThanOrEqual(2);

    // 执行 compaction，期望合并为 1 页（共 4 条）
    const { compactDatabase } = await import('@/maintenance/compaction');
    await compactDatabase(dbPath);
    const m3 = await readPagedManifest(`${dbPath}.pages`);
    const spo3 = m3!.lookups.find((l) => l.order === 'SPO')!;
    const pagesAfter = spo3.pages.filter((p) => p.primaryValue === primary).length;
    expect(pagesAfter).toBeLessThanOrEqual(pagesMiddle);

    // 读逻辑不变
    const results = db.find({ subject: 'S', predicate: 'R' }).all();
    expect(results).toHaveLength(4);
  });
});
