export declare class StringDictionary {
    private readonly valueToId;
    private readonly idToValue;
    constructor(initialValues?: string[]);
    get size(): number;
    getOrCreateId(value: string): number;
    getId(value: string): number | undefined;
    getValue(id: number): string | undefined;
    serialize(): Buffer;
    static deserialize(buffer: Buffer): StringDictionary;
}
//# sourceMappingURL=dictionary.d.ts.map