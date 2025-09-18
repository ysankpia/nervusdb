import { promises as fs } from 'node:fs';
import * as fssync from 'node:fs';
import { basename, join, dirname } from 'node:path';
import { brotliCompressSync, brotliDecompressSync, constants as zconst } from 'node:zlib';
export const DEFAULT_PAGE_SIZE = 1024; // 条目数量
export class PagedIndexWriter {
    filePath;
    pageSize;
    buffers = new Map();
    pages = [];
    compression;
    constructor(filePath, options) {
        this.filePath = filePath;
        this.pageSize = options.pageSize ?? DEFAULT_PAGE_SIZE;
        this.compression = options.compression ?? { codec: 'none' };
    }
    push(triple, primary) {
        const page = this.buffers.get(primary) ?? [];
        if (!this.buffers.has(primary)) {
            this.buffers.set(primary, page);
        }
        page.push(triple);
        if (page.length >= this.pageSize) {
            void this.flushPage(primary);
        }
    }
    async finalize() {
        for (const [primary, entries] of this.buffers.entries()) {
            if (entries.length > 0) {
                await this.flushPage(primary);
            }
        }
        this.buffers.clear();
        return [...this.pages];
    }
    async flushPage(primary) {
        const entries = this.buffers.get(primary);
        if (!entries || entries.length === 0) {
            return;
        }
        const meta = await appendTriples(this.filePath, entries, this.compression);
        this.pages.push({ primaryValue: primary, ...meta });
        entries.length = 0;
    }
}
async function appendTriples(filePath, triples, compression) {
    const handle = await fs.open(filePath, 'a');
    try {
        const buffer = Buffer.allocUnsafe(triples.length * 12);
        triples.forEach((triple, index) => {
            const offset = index * 12;
            buffer.writeUInt32LE(triple.subjectId, offset);
            buffer.writeUInt32LE(triple.predicateId, offset + 4);
            buffer.writeUInt32LE(triple.objectId, offset + 8);
        });
        const compressed = compressBuffer(buffer, compression);
        const crc = crc32(compressed);
        const stats = await handle.stat();
        const offset = stats.size;
        await handle.write(compressed, 0, compressed.length, offset);
        await handle.sync();
        return { offset, length: compressed.length, rawLength: buffer.length, crc32: crc };
    }
    finally {
        await handle.close();
    }
}
export class PagedIndexReader {
    options;
    lookup;
    filePath;
    constructor(options, lookup) {
        this.options = options;
        this.lookup = lookup;
        this.filePath = join(options.directory, pageFileName(lookup.order));
    }
    async read(primaryValue) {
        const meta = this.lookup.pages.filter((page) => page.primaryValue === primaryValue);
        if (meta.length === 0) {
            return [];
        }
        const fd = await fs.open(this.filePath, 'r');
        try {
            const result = [];
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
        }
        finally {
            await fd.close();
        }
    }
    async readAll() {
        const fd = await fs.open(this.filePath, 'r');
        try {
            const buffer = await fd.readFile();
            return deserializeTriples(buffer);
        }
        finally {
            await fd.close();
        }
    }
    readSync(primaryValue) {
        const meta = this.lookup.pages.filter((page) => page.primaryValue === primaryValue);
        if (meta.length === 0) {
            return [];
        }
        const fd = fssync.openSync(this.filePath, 'r');
        try {
            const result = [];
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
        }
        finally {
            fssync.closeSync(fd);
        }
    }
    readAllSync() {
        const buffer = fssync.readFileSync(this.filePath);
        const raw = decompressBuffer(buffer, this.options.compression);
        return deserializeTriples(raw);
    }
}
export function pageFileName(order) {
    return `${basename(order)}.idxpage`;
}
function deserializeTriples(buffer) {
    if (buffer.length === 0) {
        return [];
    }
    const count = buffer.length / 12;
    const triples = [];
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
const MANIFEST_NAME = 'index-manifest.json';
export async function writePagedManifest(directory, manifest) {
    const file = join(directory, MANIFEST_NAME);
    const tmp = `${file}.tmp`;
    // 写入紧凑 JSON，减少 I/O 体积并加快序列化
    const json = Buffer.from(JSON.stringify(manifest), 'utf8');
    const fh = await fs.open(tmp, 'w');
    try {
        await fh.write(json, 0, json.length, 0);
        await fh.sync();
    }
    finally {
        await fh.close();
    }
    await fs.rename(tmp, file);
    // fsync 父目录，确保 rename 持久化
    try {
        const dh = await fs.open(dirname(file), 'r');
        try {
            await dh.sync();
        }
        finally {
            await dh.close();
        }
    }
    catch {
        // 某些平台不支持目录 fsync，忽略
    }
}
export async function readPagedManifest(directory) {
    const file = join(directory, MANIFEST_NAME);
    try {
        const buffer = await fs.readFile(file);
        return JSON.parse(buffer.toString('utf8'));
    }
    catch {
        return null;
    }
}
function compressBuffer(input, options) {
    if (options.codec === 'none')
        return input;
    const level = clamp(options.level ?? 4, 1, 11);
    return brotliCompressSync(input, {
        params: {
            [zconst.BROTLI_PARAM_QUALITY]: level,
        },
    });
}
function decompressBuffer(input, options) {
    if (options.codec === 'none')
        return input;
    return brotliDecompressSync(input);
}
function clamp(v, min, max) {
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
function crc32(buf) {
    let c = 0xffffffff;
    for (let i = 0; i < buf.length; i += 1) {
        c = CRC32_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
    }
    return (c ^ 0xffffffff) >>> 0;
}
//# sourceMappingURL=pagedIndex.js.map