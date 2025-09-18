export class TripleStore {
    triples = [];
    keys = new Set();
    constructor(initialTriples = []) {
        initialTriples.forEach((triple) => this.add(triple));
    }
    get size() {
        return this.triples.length;
    }
    add(triple) {
        const key = encodeTripleKey(triple);
        if (this.keys.has(key)) {
            return;
        }
        this.keys.add(key);
        this.triples.push({ ...triple });
    }
    list() {
        return [...this.triples];
    }
    has(triple) {
        return this.keys.has(encodeTripleKey(triple));
    }
    serialize() {
        const countBuffer = Buffer.allocUnsafe(4);
        countBuffer.writeUInt32LE(this.triples.length, 0);
        const body = Buffer.allocUnsafe(this.triples.length * 12);
        this.triples.forEach((triple, index) => {
            const offset = index * 12;
            body.writeUInt32LE(triple.subjectId, offset);
            body.writeUInt32LE(triple.predicateId, offset + 4);
            body.writeUInt32LE(triple.objectId, offset + 8);
        });
        return Buffer.concat([countBuffer, body]);
    }
    static deserialize(buffer) {
        if (buffer.length === 0) {
            return new TripleStore();
        }
        const tripleCount = buffer.readUInt32LE(0);
        const triples = [];
        for (let i = 0; i < tripleCount; i += 1) {
            const offset = 4 + i * 12;
            triples.push({
                subjectId: buffer.readUInt32LE(offset),
                predicateId: buffer.readUInt32LE(offset + 4),
                objectId: buffer.readUInt32LE(offset + 8),
            });
        }
        return new TripleStore(triples);
    }
}
function encodeTripleKey(t) {
    return `${t.subjectId}:${t.predicateId}:${t.objectId}`;
}
//# sourceMappingURL=tripleStore.js.map