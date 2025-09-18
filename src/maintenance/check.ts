import { promises as fs } from 'node:fs';
import { join } from 'node:path';

import { readPagedManifest, pageFileName } from '../storage/pagedIndex';

export interface PageError {
  order: string;
  primaryValue: number;
  offset: number;
  length: number;
  expectedCrc?: number;
  actualCrc?: number;
  reason: string;
}

export interface StrictCheckResult {
  ok: boolean;
  errors: PageError[];
}

export async function checkStrict(dbPath: string): Promise<StrictCheckResult> {
  const indexDir = `${dbPath}.pages`;
  const manifest = await readPagedManifest(indexDir);
  const errors: PageError[] = [];
  if (!manifest) {
    return {
      ok: false,
      errors: [{ order: '*', primaryValue: -1, offset: 0, length: 0, reason: 'missing_manifest' }],
    };
  }

  for (const lookup of manifest.lookups) {
    const file = join(indexDir, pageFileName(lookup.order));
    let handle: fs.FileHandle | null = null;
    try {
      handle = await fs.open(file, 'r');
      const stat = await handle.stat();
      for (const page of lookup.pages) {
        if (page.offset + page.length > stat.size) {
          errors.push({
            order: lookup.order,
            primaryValue: page.primaryValue,
            offset: page.offset,
            length: page.length,
            reason: 'out_of_range',
          });
          continue;
        }
        const buf = Buffer.allocUnsafe(page.length);
        await handle.read(buf, 0, page.length, page.offset);
        if (page.crc32 !== undefined) {
          const actual = crc32(buf);
          if (actual !== page.crc32) {
            errors.push({
              order: lookup.order,
              primaryValue: page.primaryValue,
              offset: page.offset,
              length: page.length,
              expectedCrc: page.crc32,
              actualCrc: actual,
              reason: 'crc_mismatch',
            });
          }
        }
      }
    } catch (e) {
      errors.push({
        order: lookup.order,
        primaryValue: -1,
        offset: 0,
        length: 0,
        reason: `open_failed:${(e as Error).message}`,
      });
    } finally {
      if (handle) await handle.close();
    }
  }

  return { ok: errors.length === 0, errors };
}

// CRC32 实现（与 pagedIndex.ts 写入时一致）
const CRC32_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let c = i;
    for (let k = 0; k < 8; k += 1) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[i] = c >>> 0;
  }
  return table;
})();

function crc32(buf: Buffer): number {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i += 1) {
    c = CRC32_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}
