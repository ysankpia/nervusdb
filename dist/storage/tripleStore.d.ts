export interface EncodedTriple {
    subjectId: number;
    predicateId: number;
    objectId: number;
}
export declare class TripleStore {
    private readonly triples;
    private readonly keys;
    constructor(initialTriples?: EncodedTriple[]);
    get size(): number;
    add(triple: EncodedTriple): void;
    list(): EncodedTriple[];
    has(triple: EncodedTriple): boolean;
    serialize(): Buffer;
    static deserialize(buffer: Buffer): TripleStore;
}
//# sourceMappingURL=tripleStore.d.ts.map