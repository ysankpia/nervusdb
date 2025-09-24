import { promises as fs } from 'node:fs';
import * as fssync from 'node:fs';
import { basename, join, dirname } from 'node:path';
import { brotliCompressSync, brotliDecompressSync, constants as zconst } from 'node:zlib';

import { OrderedTriple, type IndexOrder } from './tripleIndexes.js';

export interface PageMeta {
  primaryValue: number;
  offset: number;
  length: number; // 压缩后的长度
  rawLength?: number; // 原始未压缩长度（可选）
  crc32?: number; // 压缩数据的 CRC32（可选但推荐）
}

export interface PageLookup {
  order: IndexOrder;
  pages: PageMeta[];
}

export interface PagedIndexOptions {
  directory: string;
  pageSize?: number;
  compression?: CompressionOptions;
}

export const DEFAULT_PAGE_SIZE = 1024; // 条目数量

export class PagedIndexWriter {
  private readonly pageSize: number;
  private readonly buffers = new Map<number, OrderedTriple[]>();
  private readonly pages: PageMeta[] = [];
  private readonly compression: CompressionOptions;
  private readonly pendingWrites: Array<{ primary: number; entries: OrderedTriple[] }> = [];

  constructor(
    private readonly filePath: string,
    options: PagedIndexOptions,
  ) {
    this.pageSize = options.pageSize ?? DEFAULT_PAGE_SIZE;
    this.compression = options.compression ?? { codec: 'none' };
  }

  push(triple: OrderedTriple, primary: number): void {
    const page = this.buffers.get(primary) ?? [];
    if (!this.buffers.has(primary)) {
      this.buffers.set(primary, page);
    }
    page.push(triple);

    if (page.length >= this.pageSize) {
      this.flushPageToPending(primary);
    }
  }

  async finalize(): Promise<PageMeta[]> {
    // 将所有剩余的缓冲页面添加到待写入队列
    for (const [primary, entries] of this.buffers.entries()) {
      if (entries.length > 0) {
        this.flushPageToPending(primary);
      }
    }
    this.buffers.clear();

    // 批量写入所有页面（一次打开，多次写入，一次sync）
    if (this.pendingWrites.length > 0) {
      await this.batchWritePages();
    }

    return [...this.pages];
  }

  private flushPageToPending(primary: number): void {
    const entries = this.buffers.get(primary);
    if (!entries || entries.length === 0) {
      return;
    }

    // 复制条目到待写入队列，避免引用问题
    this.pendingWrites.push({ primary, entries: [...entries] });
    entries.length = 0;
  }

  private async batchWritePages(): Promise<void> {
    const handle = await fs.open(this.filePath, 'a');
    const newPages: PageMeta[] = []; // 1. 创建临时元数据数组
    try {
      let currentOffset = (await handle.stat()).size;

      // 批量写入所有待处理的页面
      for (const { primary, entries } of this.pendingWrites) {
        const buffer = Buffer.allocUnsafe(entries.length * 12);
        entries.forEach((triple, index) => {
          const offset = index * 12;
          buffer.writeUInt32LE(triple.subjectId, offset);
          buffer.writeUInt32LE(triple.predicateId, offset + 4);
          buffer.writeUInt32LE(triple.objectId, offset + 8);
        });

        const compressed = compressBuffer(buffer, this.compression);
        const crc = crc32(compressed);

        // 写入数据（不立即sync）
        await handle.write(compressed, 0, compressed.length, currentOffset);

        // 2. 记录页面元数据到临时数组
        newPages.push({
          primaryValue: primary,
          offset: currentOffset,
          length: compressed.length,
          rawLength: buffer.length,
          crc32: crc,
        });

        currentOffset += compressed.length;
      }

      // 批量完成后只执行一次sync
      await handle.sync();

      // 3. sync 成功后，原子性地更新实例的元数据
      this.pages.push(...newPages);
    } finally {
      await handle.close();
    }

    // 清空待写入队列
    this.pendingWrites.length = 0;
  }
}

export interface PagedIndexReaderOptions {
  directory: string;
  compression: CompressionOptions;
}

export class PagedIndexReader {
  private readonly filePath: string;
  constructor(
    private readonly options: PagedIndexReaderOptions,
    private readonly lookup: PageLookup,
  ) {
    this.filePath = join(options.directory, pageFileName(lookup.order));
  }

  // 提供受控访问：返回去重后的主键列表，避免直接触达内部 lookup
  getPrimaryValues(): number[] {
    return [...new Set(this.lookup.pages.map((p) => p.primaryValue))];
  }

  async read(primaryValue: number): Promise<OrderedTriple[]> {
    const meta = this.lookup.pages.filter((page) => page.primaryValue === primaryValue);
    if (meta.length === 0) {
      return [];
    }

    const fd = await fs.open(this.filePath, 'r');
    try {
      const result: OrderedTriple[] = [];
      for (const page of meta) {
        const buffer = Buffer.allocUnsafe(page.length);
        await fd.read(buffer, 0, page.length, page.offset);
        if (page.crc32 !== undefined && page.crc32 !== crc32(buffer)) {
          // 跳过校验失败的页
          continue;
        }
        const raw = decompressBuffer(buffer, this.options.compression);
        result.push(...deserializeTriples(raw));
      }
      return result;
    } finally {
      await fd.close();
    }
  }

  async readAll(): Promise<OrderedTriple[]> {
    const fd = await fs.open(this.filePath, 'r');
    try {
      const buffer = await fd.readFile();
      return deserializeTriples(buffer);
    } finally {
      await fd.close();
    }
  }

  /**
   * 流式读取所有数据，避免一次性加载到内存
   */
  async *readAllStreaming(): AsyncIterableIterator<OrderedTriple> {
    const fd = await fs.open(this.filePath, 'r');
    try {
      // 按页分批读取，避免一次性加载整个文件
      for (const page of this.lookup.pages) {
        const buffer = Buffer.allocUnsafe(page.length);
        await fd.read(buffer, 0, page.length, page.offset);

        if (page.crc32 !== undefined && page.crc32 !== crc32(buffer)) {
          // 跳过校验失败的页
          continue;
        }

        const raw = decompressBuffer(buffer, this.options.compression);

        // 逐条解码并 yield，避免创建中间大数组
        for (const triple of iterateTriples(raw)) {
          yield triple;
        }
      }
    } finally {
      await fd.close();
    }
  }

  readSync(primaryValue: number): OrderedTriple[] {
    const meta = this.lookup.pages.filter((page) => page.primaryValue === primaryValue);
    if (meta.length === 0) {
      return [];
    }
    const fd = fssync.openSync(this.filePath, 'r');
    try {
      const result: OrderedTriple[] = [];
      for (const page of meta) {
        const buffer = Buffer.allocUnsafe(page.length);
        fssync.readSync(fd, buffer, 0, page.length, page.offset);
        if (page.crc32 !== undefined && page.crc32 !== crc32(buffer)) {
          // 跳过校验失败的页
          continue;
        }
        const raw = decompressBuffer(buffer, this.options.compression);
        result.push(...deserializeTriples(raw));
      }
      return result;
    } finally {
      fssync.closeSync(fd);
    }
  }

  readAllSync(): OrderedTriple[] {
    const buffer = fssync.readFileSync(this.filePath);
    const raw = decompressBuffer(buffer, this.options.compression);
    return deserializeTriples(raw);
  }

  // 流式迭代：逐页异步读取，避免大结果集内存压力
  async *streamByPrimaryValue(
    primaryValue: number,
  ): AsyncGenerator<OrderedTriple[], void, unknown> {
    const meta = this.lookup.pages.filter((page) => page.primaryValue === primaryValue);
    if (meta.length === 0) {
      return;
    }

    const fd = await fs.open(this.filePath, 'r');
    try {
      for (const page of meta) {
        const buffer = Buffer.allocUnsafe(page.length);
        await fd.read(buffer, 0, page.length, page.offset);
        if (page.crc32 !== undefined && page.crc32 !== crc32(buffer)) {
          // 跳过校验失败的页
          continue;
        }
        const raw = decompressBuffer(buffer, this.options.compression);
        // 将本页 triples 以批次返回，避免在内存中累积
        const batch: OrderedTriple[] = [];
        for (const t of iterateTriples(raw)) {
          batch.push(t);
        }
        if (batch.length > 0) {
          yield batch;
        }
      }
    } finally {
      await fd.close();
    }
  }

  // 流式迭代：逐页读取所有数据，支持全量查询的流式处理
  async *streamAll(): AsyncGenerator<OrderedTriple[], void, unknown> {
    // 按primaryValue分组，逐组流式读取
    const primaryValues = new Set(this.lookup.pages.map((page) => page.primaryValue));
    for (const primaryValue of primaryValues) {
      yield* this.streamByPrimaryValue(primaryValue);
    }
  }
}

export function pageFileName(order: string): string {
  return `${basename(order)}.idxpage`;
}

function deserializeTriples(buffer: Buffer): OrderedTriple[] {
  if (buffer.length === 0) {
    return [];
  }
  const count = buffer.length / 12;
  const triples: OrderedTriple[] = [];
  for (let i = 0; i < count; i += 1) {
    const offset = i * 12;
    triples.push({
      subjectId: buffer.readUInt32LE(offset),
      predicateId: buffer.readUInt32LE(offset + 4),
      objectId: buffer.readUInt32LE(offset + 8),
    });
  }
  return triples;
}

// 仅解码迭代，不创建中间数组，降低峰值内存
function* iterateTriples(buffer: Buffer): Generator<OrderedTriple, void, unknown> {
  if (buffer.length === 0) {
    return;
  }
  const count = buffer.length / 12;
  for (let i = 0; i < count; i += 1) {
    const offset = i * 12;
    yield {
      subjectId: buffer.readUInt32LE(offset),
      predicateId: buffer.readUInt32LE(offset + 4),
      objectId: buffer.readUInt32LE(offset + 8),
    };
  }
}

// Manifest for paged indexes
export interface PagedIndexManifest {
  version: number;
  pageSize: number;
  createdAt: number;
  compression: CompressionOptions;
  tombstones?: Array<[number, number, number]>; // 三元组ID的逻辑删除集合
  epoch?: number; // manifest 版本号（用于读者可见性/运维）
  orphans?: Array<{ order: IndexOrder; pages: PageMeta[] }>; // 增量重写后不再被引用的旧页（待 GC）
  lookups: PageLookup[];
}

const MANIFEST_NAME = 'index-manifest.json';

export async function writePagedManifest(
  directory: string,
  manifest: PagedIndexManifest,
): Promise<void> {
  const file = join(directory, MANIFEST_NAME);
  // 使用唯一的临时文件名，避免并发写入时的冲突（同一目录下可能并发调用）
  const tmp = `${file}.tmp-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  // 写入紧凑 JSON，减少 I/O 体积并加快序列化
  const json = Buffer.from(JSON.stringify(manifest), 'utf8');

  const fh = await fs.open(tmp, 'w');
  try {
    await fh.write(json, 0, json.length, 0);
    await fh.sync();
  } finally {
    await fh.close();
  }
  await fs.rename(tmp, file);
  // fsync 父目录，确保 rename 持久化
  try {
    const dh = await fs.open(dirname(file), 'r');
    try {
      await dh.sync();
    } finally {
      await dh.close();
    }
  } catch {
    // 某些平台不支持目录 fsync，忽略
  }
}

export async function readPagedManifest(directory: string): Promise<PagedIndexManifest | null> {
  const file = join(directory, MANIFEST_NAME);
  try {
    const buffer = await fs.readFile(file);
    return JSON.parse(buffer.toString('utf8')) as PagedIndexManifest;
  } catch {
    return null;
  }
}

// 压缩配置与实现
export type CompressionCodec = 'none' | 'brotli';

export interface CompressionOptions {
  codec: CompressionCodec;
  level?: number; // Brotli 等级：1-11（默认使用 4）
}

function compressBuffer(input: Buffer, options: CompressionOptions): Buffer {
  if (options.codec === 'none') return input;
  const level = clamp(options.level ?? 4, 1, 11);
  return brotliCompressSync(input, {
    params: {
      [zconst.BROTLI_PARAM_QUALITY]: level,
    },
  });
}

function decompressBuffer(input: Buffer, options: CompressionOptions): Buffer {
  if (options.codec === 'none') return input;
  return brotliDecompressSync(input);
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

// 轻量 CRC32（polynomial 0xEDB88320）
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
