import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, readFile, readdir, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { SynapseDB } from '@/synapseDb';
import { autoCompact } from '@/maintenance/autoCompact';

describe('Auto-Compact 自动并入 LSM 段并清理', () => {
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-ac-lsm-'));
    dbPath = join(workspace, 'acl.synapsedb');
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

  it('includeLsmSegmentsAuto 触发阈值时自动并入并清空清单', async () => {
    const db = await SynapseDB.open(dbPath, { stagingMode: 'lsm-lite' as any, pageSize: 2 });
    db.addFact({ subject: 'S', predicate: 'R', object: 'O1' });
    await db.flush();
    const manPath = `${dbPath}.pages/lsm-manifest.json`;
    const m1 = JSON.parse(await readFile(manPath, 'utf8')) as { segments: any[] };
    expect(m1.segments.length).toBeGreaterThan(0);

    const decision = await autoCompact(dbPath, {
      orders: ['SPO'],
      mode: 'rewrite',
      includeLsmSegmentsAuto: true,
      lsmSegmentsThreshold: 1,
      dryRun: false,
    });
    expect(decision.selectedOrders).toContain('SPO');
    const m2 = JSON.parse(await readFile(manPath, 'utf8')) as { segments: any[] };
    expect((m2.segments ?? []).length).toBe(0);
  });
});
