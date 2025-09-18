import { TextDecoder, TextEncoder } from 'node:util';
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf8');
export class StringDictionary {
    valueToId = new Map();
    idToValue = [];
    constructor(initialValues = []) {
        initialValues.forEach((value) => {
            this.getOrCreateId(value);
        });
    }
    get size() {
        return this.idToValue.length;
    }
    getOrCreateId(value) {
        const existing = this.valueToId.get(value);
        if (existing !== undefined) {
            return existing;
        }
        const id = this.idToValue.length;
        this.idToValue.push(value);
        this.valueToId.set(value, id);
        return id;
    }
    getId(value) {
        return this.valueToId.get(value);
    }
    getValue(id) {
        return this.idToValue[id];
    }
    serialize() {
        const buffers = [];
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
    static deserialize(buffer) {
        if (buffer.length === 0) {
            return new StringDictionary();
        }
        let offset = 0;
        const readUInt32 = () => {
            const value = buffer.readUInt32LE(offset);
            offset += 4;
            return value;
        };
        const entryCount = readUInt32();
        const values = [];
        for (let i = 0; i < entryCount; i += 1) {
            const length = readUInt32();
            const slice = buffer.subarray(offset, offset + length);
            offset += length;
            values.push(decoder.decode(slice));
        }
        return new StringDictionary(values);
    }
}
//# sourceMappingURL=dictionary.js.map