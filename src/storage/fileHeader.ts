import { promises as fs } from 'node:fs';
import { dirname } from 'node:path';
import {
  FILE_HEADER_LENGTH,
  FILE_VERSION,
  MAGIC_HEADER,
  createEmptyLayout,
  FileHeader,
  FileLayout,
  SectionPointer,
} from './layout.js';

const UINT32_BYTES = 4;

function encodeHeader(layout: FileLayout): Buffer {
  const buffer = Buffer.alloc(FILE_HEADER_LENGTH, 0);
  MAGIC_HEADER.copy(buffer, 0);
  buffer.writeUInt32LE(FILE_VERSION, MAGIC_HEADER.length);

  const writeSection = (section: SectionPointer, index: number) => {
    const base = 16 + index * UINT32_BYTES * 2;
    buffer.writeUInt32LE(section.offset, base);
    buffer.writeUInt32LE(section.length, base + UINT32_BYTES);
  };

  writeSection(layout.dictionary, 0);
  writeSection(layout.triples, 1);
  writeSection(layout.indexes, 2);
  writeSection(layout.properties, 3);

  return buffer;
}

function decodeHeader(buffer: Buffer): FileHeader {
  const magic = buffer.subarray(0, MAGIC_HEADER.length);
  if (!magic.equals(MAGIC_HEADER)) {
    throw new Error('非法的 NervusDB 文件头');
  }

  const version = buffer.readUInt32LE(MAGIC_HEADER.length);
  if (version !== FILE_VERSION) {
    throw new Error(`暂不支持的文件版本: ${version}`);
  }

  const readSection = (index: number): SectionPointer => {
    const base = 16 + index * UINT32_BYTES * 2;
    return {
      offset: buffer.readUInt32LE(base),
      length: buffer.readUInt32LE(base + UINT32_BYTES),
    };
  };

  return {
    magic,
    version,
    layout: {
      dictionary: readSection(0),
      triples: readSection(1),
      indexes: readSection(2),
      properties: readSection(3),
    },
  };
}

export interface SerializedSections {
  dictionary: Buffer;
  triples: Buffer;
  indexes?: Buffer;
  properties?: Buffer;
}

export async function writeStorageFile(path: string, sections: SerializedSections): Promise<void> {
  const indexes = sections.indexes ?? Buffer.alloc(0);
  const properties = sections.properties ?? Buffer.alloc(0);

  const layout = createEmptyLayout();
  layout.dictionary = {
    offset: FILE_HEADER_LENGTH,
    length: sections.dictionary.length,
  };
  layout.triples = {
    offset: layout.dictionary.offset + layout.dictionary.length,
    length: sections.triples.length,
  };
  layout.indexes = {
    offset: layout.triples.offset + layout.triples.length,
    length: indexes.length,
  };
  layout.properties = {
    offset: layout.indexes.offset + layout.indexes.length,
    length: properties.length,
  };

  const header = encodeHeader(layout);
  const body = Buffer.concat([sections.dictionary, sections.triples, indexes, properties]);

  // crash-safe：写入临时文件 → fsync → rename → fsync 目录
  const tmp = `${path}.tmp`;
  const fh = await fs.open(tmp, 'w');
  try {
    const content = Buffer.concat([header, body]);
    await fh.write(content, 0, content.length, 0);
    await fh.sync();
  } finally {
    await fh.close();
  }
  await fs.rename(tmp, path);
  // fsync 父目录，确保 rename 落盘
  const dir = dirname(path);
  try {
    const dh = await fs.open(dir, 'r');
    try {
      await dh.sync();
    } finally {
      await dh.close();
    }
  } catch {
    // 某些平台不支持目录 fsync，忽略
  }
}

export interface LoadedSections {
  header: FileHeader;
  dictionary: Buffer;
  triples: Buffer;
  indexes: Buffer;
  properties: Buffer;
}

export async function readStorageFile(path: string): Promise<LoadedSections> {
  const file = await fs.readFile(path);
  if (file.length < FILE_HEADER_LENGTH) {
    throw new Error('NervusDB 文件长度不足');
  }

  const headerBuffer = file.subarray(0, FILE_HEADER_LENGTH);
  const header = decodeHeader(headerBuffer);

  const readSection = (section: SectionPointer): Buffer => {
    const { offset, length } = section;
    if (length === 0) {
      return Buffer.alloc(0);
    }
    return file.subarray(offset, offset + length);
  };

  return {
    header,
    dictionary: readSection(header.layout.dictionary),
    triples: readSection(header.layout.triples),
    indexes: readSection(header.layout.indexes),
    properties: readSection(header.layout.properties),
  };
}

export async function initializeIfMissing(path: string): Promise<void> {
  try {
    await fs.access(path);
  } catch {
    const emptySections: SerializedSections = {
      dictionary: Buffer.alloc(4, 0),
      triples: Buffer.alloc(4, 0),
      indexes: Buffer.alloc(0),
      properties: Buffer.alloc(0),
    };
    emptySections.dictionary.writeUInt32LE(0, 0);
    emptySections.triples.writeUInt32LE(0, 0);
    await writeStorageFile(path, emptySections);
  }
}
