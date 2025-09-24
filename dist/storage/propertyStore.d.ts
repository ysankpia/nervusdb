export interface TripleKey {
    subjectId: number;
    predicateId: number;
    objectId: number;
}
export declare class PropertyStore {
    private readonly nodeProperties;
    private readonly edgeProperties;
    setNodeProperties(nodeId: number, value: Record<string, unknown>): void;
    getNodeProperties<T extends Record<string, unknown>>(nodeId: number): T | undefined;
    setEdgeProperties(key: TripleKey, value: Record<string, unknown>): void;
    getEdgeProperties<T extends Record<string, unknown>>(key: TripleKey): T | undefined;
    serialize(): Buffer;
    static deserialize(buffer: Buffer): PropertyStore;
    /**
     * 获取所有节点属性数据（用于重建索引）
     */
    getAllNodeProperties(): Map<number, Record<string, unknown>>;
    /**
     * 获取所有边属性数据（用于重建索引）
     */
    getAllEdgeProperties(): Map<string, Record<string, unknown>>;
}
//# sourceMappingURL=propertyStore.d.ts.map