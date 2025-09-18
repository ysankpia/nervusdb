export type IndexOrder = 'SPO' | 'POS' | 'OSP' | 'SOP' | 'PSO' | 'OPS';
export interface OrderedTriple {
    subjectId: number;
    predicateId: number;
    objectId: number;
}
export declare class TripleIndexes {
    private readonly indexes;
    constructor(initialData?: Map<IndexOrder, OrderedTriple[]>);
    seed(triples: OrderedTriple[]): void;
    add(triple: OrderedTriple): void;
    get(order: IndexOrder): OrderedTriple[];
    query(criteria: Partial<OrderedTriple>): OrderedTriple[];
    serialize(): Buffer;
    static deserialize(buffer: Buffer): TripleIndexes;
    private insertIntoBuckets;
}
export declare function getBestIndexKey(criteria: Partial<OrderedTriple>): IndexOrder;
//# sourceMappingURL=tripleIndexes.d.ts.map