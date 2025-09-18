import { promises as fs } from 'node:fs';
import * as fssync from 'node:fs';

export type WalRecordType =
  | 0x10 // addTriple
  | 0x20 // deleteTriple
  | 0x30 // setNodeProps
  | 0x31 // setEdgeProps
  | 0x40 // beginBatch
  | 0x41 // commitBatch
  | 0x42; // abortBatch

export interface FactInput {
  subject: string;
  predicate: string;
  object: string;
}

const MAGIC = Buffer.from('SYNWAL', 'utf8');
const WAL_VERSION = 2;

export class WalWriter {
  private constructor(
    private readonly walPath: string,
    private fd: fs.FileHandle,
    private offset: number,
  ) {}
  static async open(dbPath: string): Promise<WalWriter> {
    const walPath = `${dbPath}.wal`;
    let fd: fs.FileHandle;
    let offset = 0;
    try {
      fd = await fs.open(walPath, 'r+');
      const header = Buffer.alloc(12);
      await fd.read(header, 0, 12, 0);
      if (header.length < 12 || !header.subarray(0, 6).equals(MAGIC)) {
        await fd.truncate(0);
        await writeHeader(fd);
        offset = 12;
      } else {
        const stat = await fd.stat();
        offset = stat.size;
      }
    } catch {
      fd = await fs.open(walPath, 'w+');
      await writeHeader(fd);
      offset = 12;
    }
    return new WalWriter(walPath, fd, offset);
  }

  async appendAddTriple(fact: FactInput): Promise<void> {
    const payload = encodeStrings([fact.subject, fact.predicate, fact.object]);
    this.writeRecordSync(0x10, payload);
  }

  async appendDeleteTriple(fact: FactInput): Promise<void> {
    const payload = encodeStrings([fact.subject, fact.predicate, fact.object]);
    this.writeRecordSync(0x20, payload);
  }

  async appendSetNodeProps(nodeId: number, props: unknown): Promise<void> {
    const body = Buffer.from(JSON.stringify(props ?? {}), 'utf8');
    const buf = Buffer.allocUnsafe(4 + 4 + body.length);
    buf.writeUInt32LE(nodeId, 0);
    buf.writeUInt32LE(body.length, 4);
    body.copy(buf, 8);
    this.writeRecordSync(0x30, buf);
  }

  async appendSetEdgeProps(
    ids: { subjectId: number; predicateId: number; objectId: number },
    props: unknown,
  ): Promise<void> {
    const body = Buffer.from(JSON.stringify(props ?? {}), 'utf8');
    const buf = Buffer.allocUnsafe(12 + 4 + body.length);
    buf.writeUInt32LE(ids.subjectId, 0);
    buf.writeUInt32LE(ids.predicateId, 4);
    buf.writeUInt32LE(ids.objectId, 8);
    buf.writeUInt32LE(body.length, 12);
    body.copy(buf, 16);
    this.writeRecordSync(0x31, buf);
  }

  async appendBegin(): Promise<void> {
    this.writeRecordSync(0x40, Buffer.alloc(0));
  }

  async appendCommit(): Promise<void> {
    this.writeRecordSync(0x41, Buffer.alloc(0));
  }

  async appendAbort(): Promise<void> {
    this.writeRecordSync(0x42, Buffer.alloc(0));
  }

  async reset(): Promise<void> {
    await this.fd.truncate(0);
    await writeHeader(this.fd);
    this.offset = 12;
  }

  async truncateTo(offset: number): Promise<void> {
    await this.fd.truncate(offset);
    this.offset = offset;
  }

  async close(): Promise<void> {
    await this.fd.close();
  }

  private writeRecordSync(type: WalRecordType, payload: Buffer): void {
    const fixed = Buffer.alloc(9);
    fixed.writeUInt8(type, 0);
    fixed.writeUInt32LE(payload.length, 1);
    fixed.writeUInt32LE(simpleChecksum(payload), 5);
    // 使用同步写，避免跨实例读取竞态
    const fdnum = (this.fd as unknown as { fd: number }).fd;
    fssync.writeSync(fdnum, fixed, 0, fixed.length, this.offset);
    fssync.writeSync(fdnum, payload, 0, payload.length, this.offset + fixed.length);
    this.offset += fixed.length + payload.length;
  }
}

export class WalReplayer {
  constructor(private readonly dbPath: string) {}

  async replay(): Promise<{
    addFacts: FactInput[];
    deleteFacts: FactInput[];
    nodeProps: Array<{ nodeId: number; value: unknown }>;
    edgeProps: Array<{
      ids: { subjectId: number; predicateId: number; objectId: number };
      value: unknown;
    }>;
    safeOffset: number;
    version: number;
  }> {
    const walPath = `${this.dbPath}.wal`;
    let fh: fs.FileHandle | null = null;
    const addFacts: FactInput[] = [];
    const deleteFacts: FactInput[] = [];
    const nodeProps: Array<{ nodeId: number; value: unknown }> = [];
    const edgeProps: Array<{
      ids: { subjectId: number; predicateId: number; objectId: number };
      value: unknown;
    }> = [];
    let safeOffset = 0;
    let version = 0;
    try {
      fh = await fs.open(walPath, 'r');
    } catch {
      return { addFacts, deleteFacts, nodeProps, edgeProps, safeOffset: 0, version: WAL_VERSION };
    }
    try {
      const stat = await fh.stat();
      if (stat.size < 12)
        return { addFacts, deleteFacts, nodeProps, edgeProps, safeOffset: stat.size, version };
      const header = Buffer.alloc(12);
      await fh.read(header, 0, 12, 0);
      if (!header.subarray(0, 6).equals(MAGIC)) {
        return { addFacts, deleteFacts, nodeProps, edgeProps, safeOffset: 0, version };
      }

      version = header.readUInt32LE(6);
      let offset = 12;
      safeOffset = offset;

      let inBatch = false;
      let stagedAdd: FactInput[] = [];
      let stagedDel: FactInput[] = [];
      let stagedNode: Array<{ nodeId: number; value: unknown }> = [];
      let stagedEdge: Array<{
        ids: { subjectId: number; predicateId: number; objectId: number };
        value: unknown;
      }> = [];
      while (offset + 9 <= stat.size) {
        const fixed = Buffer.alloc(9); // type(1) + len(4) + checksum(4)
        await fh.read(fixed, 0, 9, offset);
        const type = fixed.readUInt8(0) as WalRecordType;
        const length = fixed.readUInt32LE(1);
        const checksum = fixed.readUInt32LE(5);
        offset += 9;
        if (length < 0 || offset + length > stat.size) break; // incomplete
        const payload = Buffer.alloc(length);
        await fh.read(payload, 0, length, offset);
        offset += length;

        if (simpleChecksum(payload) !== checksum) {
          // checksum mismatch, stop
          break;
        }
        safeOffset = offset;

        if (type === 0x40) {
          inBatch = true;
          stagedAdd = [];
          stagedDel = [];
          stagedNode = [];
          stagedEdge = [];
        } else if (type === 0x41) {
          // commit
          addFacts.push(...stagedAdd);
          deleteFacts.push(...stagedDel);
          nodeProps.push(...stagedNode);
          edgeProps.push(...stagedEdge);
          inBatch = false;
          stagedAdd = [];
          stagedDel = [];
          stagedNode = [];
          stagedEdge = [];
        } else if (type === 0x42) {
          // abort，丢弃暂存
          inBatch = false;
          stagedAdd = [];
          stagedDel = [];
          stagedNode = [];
          stagedEdge = [];
        } else if (type === 0x10) {
          const [subject, predicate, object] = decodeStrings(payload);
          if (version >= 2 && inBatch) stagedAdd.push({ subject, predicate, object });
          else addFacts.push({ subject, predicate, object });
        } else if (type === 0x20) {
          const [subject, predicate, object] = decodeStrings(payload);
          if (version >= 2 && inBatch) stagedDel.push({ subject, predicate, object });
          else deleteFacts.push({ subject, predicate, object });
        } else if (type === 0x30) {
          const nodeId = payload.readUInt32LE(0);
          const len = payload.readUInt32LE(4);
          const json = payload.subarray(8, 8 + len).toString('utf8');
          const item = { nodeId, value: safeParse(json) };
          if (version >= 2 && inBatch) stagedNode.push(item);
          else nodeProps.push(item);
        } else if (type === 0x31) {
          const subjectId = payload.readUInt32LE(0);
          const predicateId = payload.readUInt32LE(4);
          const objectId = payload.readUInt32LE(8);
          const len = payload.readUInt32LE(12);
          const json = payload.subarray(16, 16 + len).toString('utf8');
          const item = { ids: { subjectId, predicateId, objectId }, value: safeParse(json) };
          if (version >= 2 && inBatch) stagedEdge.push(item);
          else edgeProps.push(item);
        }
      }
    } finally {
      await fh.close();
    }
    return { addFacts, deleteFacts, nodeProps, edgeProps, safeOffset, version };
  }
}

async function writeHeader(fd: fs.FileHandle): Promise<void> {
  const header = Buffer.alloc(12);
  MAGIC.copy(header, 0);
  header.writeUInt32LE(WAL_VERSION, 6);
  await fd.write(header, 0, header.length, 0);
}

// 保留空实现占位，以兼容历史引用（当前未使用）
async function writeRecord(_fd: fs.FileHandle, _type: WalRecordType, _payload: Buffer): Promise<void> {}

function encodeStrings(values: string[]): Buffer {
  const parts: Buffer[] = [];
  for (const s of values) {
    const b = Buffer.from(s, 'utf8');
    const len = Buffer.alloc(4);
    len.writeUInt32LE(b.length, 0);
    parts.push(len, b);
  }
  return Buffer.concat(parts);
}

function decodeStrings(buf: Buffer): string[] {
  const out: string[] = [];
  let off = 0;
  while (off + 4 <= buf.length) {
    const len = buf.readUInt32LE(off);
    off += 4;
    if (off + len > buf.length) break;
    out.push(buf.subarray(off, off + len).toString('utf8'));
    off += len;
  }
  return out;
}

function simpleChecksum(buf: Buffer): number {
  let sum = 0 >>> 0;
  for (let i = 0; i < buf.length; i += 1) {
    sum = (sum + buf[i]) >>> 0;
  }
  return sum >>> 0;
}

function safeParse(json: string): unknown {
  try {
    return JSON.parse(json);
  } catch {
    return {};
  }
}
