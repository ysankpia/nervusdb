function encodeJson(value, prev) {
    let version = 0;
    if (prev) {
        const parsed = safeParse(prev);
        if (parsed &&
            typeof parsed === 'object' &&
            Object.prototype.hasOwnProperty.call(parsed, '__v') &&
            typeof parsed.__v === 'number') {
            version = Number(parsed.__v ?? 0) + 1;
        }
    }
    const json = JSON.stringify({ __v: version, data: value ?? {} });
    return Buffer.from(json, 'utf8');
}
function decodeJson(buffer) {
    if (buffer.length === 0)
        return {};
    const parsed = safeParse(buffer);
    if (parsed && typeof parsed === 'object' && isWithData(parsed)) {
        return parsed.data;
    }
    return parsed;
}
function safeParse(buffer) {
    const s = buffer.toString('utf8');
    try {
        return JSON.parse(s);
    }
    catch {
        return {};
    }
}
function isWithData(obj) {
    return Object.prototype.hasOwnProperty.call(obj, 'data');
}
export class PropertyStore {
    nodeProperties = new Map();
    edgeProperties = new Map();
    setNodeProperties(nodeId, value) {
        const prev = this.nodeProperties.get(nodeId);
        this.nodeProperties.set(nodeId, encodeJson(value, prev));
    }
    getNodeProperties(nodeId) {
        const serialized = this.nodeProperties.get(nodeId);
        if (!serialized) {
            return undefined;
        }
        return decodeJson(serialized);
    }
    setEdgeProperties(key, value) {
        const k = encodeTripleKey(key);
        const prev = this.edgeProperties.get(k);
        this.edgeProperties.set(k, encodeJson(value, prev));
    }
    getEdgeProperties(key) {
        const serialized = this.edgeProperties.get(encodeTripleKey(key));
        if (!serialized) {
            return undefined;
        }
        return decodeJson(serialized);
    }
    serialize() {
        const buffers = [];
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
    static deserialize(buffer) {
        if (buffer.length === 0) {
            return new PropertyStore();
        }
        const store = new PropertyStore();
        let offset = 0;
        const readUInt32 = () => {
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
            store.edgeProperties.set(encodeTripleKey({ subjectId, predicateId, objectId }), Buffer.from(slice));
        }
        return store;
    }
}
function encodeTripleKey({ subjectId, predicateId, objectId }) {
    return `${subjectId}:${predicateId}:${objectId}`;
}
function decodeTripleKey(key) {
    const [subjectId, predicateId, objectId] = key.split(':').map((value) => Number(value));
    return { subjectId, predicateId, objectId };
}
//# sourceMappingURL=propertyStore.js.map