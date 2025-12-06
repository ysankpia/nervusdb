import { describe, it, expect } from 'vitest';
import { promises as fs } from 'node:fs';
import { join } from 'node:path';
import { checkStrict } from '@/maintenance/check.ts';
import { writePagedManifest, type PagedIndexManifest } from '@/core/storage/pagedIndex.ts';
import { makeWorkspace, within, cleanupWorkspace } from '@/../tests/helpers/tempfs.ts';

describe('maintenance.checkStrict · 清单缺失与打开失败', () => {
  it('missing manifest 返回 ok=false & missing_manifest', async () => {
    const dir = await makeWorkspace('check-missing');
    try {
      const res = await checkStrict(join(dir, 'db.synapsedb'));
      expect(res.ok).toBe(false);
      expect(res.errors[0].reason).toBe('missing_manifest');
    } finally {
      await cleanupWorkspace(dir);
    }
  });

  it('manifest 存在但页文件缺失 → open_failed', async () => {
    const dir = await makeWorkspace('check-open-failed');
    try {
      const pagesDir = within(dir, 'db.synapsedb.pages');
      await fs.mkdir(pagesDir, { recursive: true });
      const manifest: PagedIndexManifest = {
        version: 1,
        pageSize: 2,
        createdAt: Date.now(),
        compression: { codec: 'none' },
        lookups: [
          {
            order: 'SPO' as any,
            pages: [{ primaryValue: 0, offset: 0, length: 10, crc32: 123 }],
          },
        ],
      };
      await writePagedManifest(pagesDir, manifest);

      const res = await checkStrict(join(dir, 'db.synapsedb'));
      expect(res.ok).toBe(false);
      expect(res.errors.length).toBeGreaterThan(0);
      // 第一个错误应来自无法打开页文件
      expect(res.errors[0].reason.startsWith('open_failed:')).toBe(true);
    } finally {
      await cleanupWorkspace(dir);
    }
  });
});
