import { TextDecoder, TextEncoder } from 'node:util';

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf8');

export class StringDictionary {
  private readonly valueToId = new Map<string, number>();
  private readonly idToValue: string[] = [];

  constructor(initialValues: string[] = []) {
    initialValues.forEach((value) => {
      this.getOrCreateId(value);
    });
  }

  get size(): number {
    return this.idToValue.length;
  }

  getOrCreateId(value: string): number {
    const existing = this.valueToId.get(value);
    if (existing !== undefined) {
      return existing;
    }

    const id = this.idToValue.length;
    this.idToValue.push(value);
    this.valueToId.set(value, id);
    return id;
  }

  getId(value: string): number | undefined {
    return this.valueToId.get(value);
  }

  getValue(id: number): string | undefined {
    return this.idToValue[id];
  }

  serialize(): Buffer {
    const buffers: Buffer[] = [];
    const countBuffer = Buffer.allocUnsafe(4);
    countBuffer.writeUInt32LE(this.idToValue.length, 0);
    buffers.push(countBuffer);

    for (const value of this.idToValue) {
      const encoded = Buffer.from(encoder.encode(value));
      const lengthBuffer = Buffer.allocUnsafe(4);
      lengthBuffer.writeUInt32LE(encoded.length, 0);
      buffers.push(lengthBuffer, encoded);
    }

    return Buffer.concat(buffers);
  }

  static deserialize(buffer: Buffer): StringDictionary {
    if (buffer.length === 0) {
      return new StringDictionary();
    }

    let offset = 0;
    const readUInt32 = (): number => {
      const value = buffer.readUInt32LE(offset);
      offset += 4;
      return value;
    };

    const entryCount = readUInt32();
    const values: string[] = [];

    for (let i = 0; i < entryCount; i += 1) {
      const length = readUInt32();
      const slice = buffer.subarray(offset, offset + length);
      offset += length;
      values.push(decoder.decode(slice));
    }

    return new StringDictionary(values);
  }
}
