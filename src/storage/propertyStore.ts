export interface TripleKey {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

function encodeJson(value: unknown, prev?: Buffer): Buffer {
  let version = 0;
  if (prev) {
    const parsed = safeParse(prev) as { __v?: number } | Record<string, unknown>;
    if (
      parsed &&
      typeof parsed === 'object' &&
      Object.prototype.hasOwnProperty.call(parsed, '__v') &&
      typeof (parsed as { __v?: unknown }).__v === 'number'
    ) {
      version = Number((parsed as { __v?: number }).__v ?? 0) + 1;
    }
  }
  const json = JSON.stringify({ __v: version, data: value ?? {} });
  return Buffer.from(json, 'utf8');
}

function decodeJson(buffer: Buffer): unknown {
  if (buffer.length === 0) return {};
  const parsed = safeParse(buffer) as Record<string, unknown> | { data?: unknown };
  if (parsed && typeof parsed === 'object' && isWithData(parsed as Record<string, unknown>)) {
    return (parsed as { data?: unknown }).data;
  }
  return parsed;
}

function safeParse(buffer: Buffer): unknown {
  const s = buffer.toString('utf8');
  try {
    return JSON.parse(s);
  } catch {
    return {};
  }
}

function isWithData(obj: Record<string, unknown>): obj is { data?: unknown } {
  return Object.prototype.hasOwnProperty.call(obj, 'data');
}

export class PropertyStore {
  private readonly nodeProperties = new Map<number, Buffer>();
  private readonly edgeProperties = new Map<string, Buffer>();

  setNodeProperties(nodeId: number, value: Record<string, unknown>): void {
    const prev = this.nodeProperties.get(nodeId);
    this.nodeProperties.set(nodeId, encodeJson(value, prev));
  }

  getNodeProperties<T extends Record<string, unknown>>(nodeId: number): T | undefined {
    const serialized = this.nodeProperties.get(nodeId);
    if (!serialized) {
      return undefined;
    }
    return decodeJson(serialized) as T;
  }

  setEdgeProperties(key: TripleKey, value: Record<string, unknown>): void {
    const k = encodeTripleKey(key);
    const prev = this.edgeProperties.get(k);
    this.edgeProperties.set(k, encodeJson(value, prev));
  }

  getEdgeProperties<T extends Record<string, unknown>>(key: TripleKey): T | undefined {
    const serialized = this.edgeProperties.get(encodeTripleKey(key));
    if (!serialized) {
      return undefined;
    }
    return decodeJson(serialized) as T;
  }

  serialize(): Buffer {
    const buffers: Buffer[] = [];

    const nodeCount = Buffer.allocUnsafe(4);
    nodeCount.writeUInt32LE(this.nodeProperties.size, 0);
    buffers.push(nodeCount);

    for (const [nodeId, data] of this.nodeProperties.entries()) {
      const entryHeader = Buffer.allocUnsafe(8);
      entryHeader.writeUInt32LE(nodeId, 0);
      entryHeader.writeUInt32LE(data.length, 4);
      buffers.push(entryHeader, data);
    }

    const edgeCount = Buffer.allocUnsafe(4);
    edgeCount.writeUInt32LE(this.edgeProperties.size, 0);
    buffers.push(edgeCount);

    for (const [key, data] of this.edgeProperties.entries()) {
      const { subjectId, predicateId, objectId } = decodeTripleKey(key);
      const entryHeader = Buffer.allocUnsafe(16);
      entryHeader.writeUInt32LE(subjectId, 0);
      entryHeader.writeUInt32LE(predicateId, 4);
      entryHeader.writeUInt32LE(objectId, 8);
      entryHeader.writeUInt32LE(data.length, 12);
      buffers.push(entryHeader, data);
    }

    return Buffer.concat(buffers);
  }

  static deserialize(buffer: Buffer): PropertyStore {
    if (buffer.length === 0) {
      return new PropertyStore();
    }

    const store = new PropertyStore();
    let offset = 0;

    const readUInt32 = (): number => {
      const value = buffer.readUInt32LE(offset);
      offset += 4;
      return value;
    };

    const nodeCount = readUInt32();
    for (let i = 0; i < nodeCount; i += 1) {
      const nodeId = readUInt32();
      const length = readUInt32();
      const slice = buffer.subarray(offset, offset + length);
      offset += length;
      store.nodeProperties.set(nodeId, Buffer.from(slice));
    }

    const edgeCount = readUInt32();
    for (let i = 0; i < edgeCount; i += 1) {
      const subjectId = readUInt32();
      const predicateId = readUInt32();
      const objectId = readUInt32();
      const length = readUInt32();
      const slice = buffer.subarray(offset, offset + length);
      offset += length;
      store.edgeProperties.set(
        encodeTripleKey({ subjectId, predicateId, objectId }),
        Buffer.from(slice),
      );
    }

    return store;
  }

  /**
   * 获取所有节点属性数据（用于重建索引）
   */
  getAllNodeProperties(): Map<number, Record<string, unknown>> {
    const result = new Map<number, Record<string, unknown>>();
    for (const [nodeId, buffer] of this.nodeProperties.entries()) {
      const properties = decodeJson(buffer) as Record<string, unknown>;
      if (properties && Object.keys(properties).length > 0) {
        result.set(nodeId, properties);
      }
    }
    return result;
  }

  /**
   * 获取所有边属性数据（用于重建索引）
   */
  getAllEdgeProperties(): Map<string, Record<string, unknown>> {
    const result = new Map<string, Record<string, unknown>>();
    for (const [edgeKey, buffer] of this.edgeProperties.entries()) {
      const properties = decodeJson(buffer) as Record<string, unknown>;
      if (properties && Object.keys(properties).length > 0) {
        result.set(edgeKey, properties);
      }
    }
    return result;
  }
}

function encodeTripleKey({ subjectId, predicateId, objectId }: TripleKey): string {
  return `${subjectId}:${predicateId}:${objectId}`;
}

function decodeTripleKey(key: string): TripleKey {
  const [subjectId, predicateId, objectId] = key.split(':').map((value) => Number(value));
  return { subjectId, predicateId, objectId };
}
