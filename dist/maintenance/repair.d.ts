export declare function repairCorruptedOrders(dbPath: string): Promise<{
    repairedOrders: string[];
}>;
export declare function repairCorruptedPagesFast(dbPath: string): Promise<{
    repaired: Array<{
        order: string;
        primaryValues: number[];
    }>;
}>;
//# sourceMappingURL=repair.d.ts.map