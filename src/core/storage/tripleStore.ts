export interface EncodedTriple {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

export class TripleStore {
  private readonly triples: EncodedTriple[] = [];
  private readonly keys = new Set<string>();
  private version = 0;
  private seeding = false;

  constructor(initialTriples: EncodedTriple[] = []) {
    if (initialTriples.length > 0) {
      this.seeding = true;
      initialTriples.forEach((triple) => this.add(triple));
      this.seeding = false;
      this.version = 0;
    }
  }

  get size(): number {
    return this.triples.length;
  }

  add(triple: EncodedTriple): void {
    const key = encodeTripleKey(triple);
    if (this.keys.has(key)) {
      return;
    }
    this.keys.add(key);
    this.triples.push({ ...triple });
    if (!this.seeding) {
      this.version += 1;
    }
  }

  list(): EncodedTriple[] {
    return [...this.triples];
  }

  has(triple: EncodedTriple): boolean {
    return this.keys.has(encodeTripleKey(triple));
  }

  serialize(): Buffer {
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

  getVersion(): number {
    return this.version;
  }

  static deserialize(buffer: Buffer): TripleStore {
    if (buffer.length === 0) {
      return new TripleStore();
    }

    const tripleCount = buffer.readUInt32LE(0);
    const triples: EncodedTriple[] = [];
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

function encodeTripleKey(t: EncodedTriple): string {
  return `${t.subjectId}:${t.predicateId}:${t.objectId}`;
}
