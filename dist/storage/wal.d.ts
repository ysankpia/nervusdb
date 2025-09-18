export type WalRecordType = 0x10 | 0x20 | 0x30 | 0x31 | 0x40 | 0x41 | 0x42;
export interface FactInput {
    subject: string;
    predicate: string;
    object: string;
}
export interface WalBeginMeta {
    txId?: string;
    sessionId?: string;
}
export declare class WalWriter {
    private readonly walPath;
    private fd;
    private offset;
    private constructor();
    static open(dbPath: string): Promise<WalWriter>;
    appendAddTriple(fact: FactInput): void;
    appendDeleteTriple(fact: FactInput): void;
    appendSetNodeProps(nodeId: number, props: unknown): void;
    appendSetEdgeProps(ids: {
        subjectId: number;
        predicateId: number;
        objectId: number;
    }, props: unknown): void;
    appendBegin(meta?: WalBeginMeta): void;
    appendCommit(): void;
    appendCommitDurable(): Promise<void>;
    appendAbort(): void;
    reset(): Promise<void>;
    truncateTo(offset: number): Promise<void>;
    close(): Promise<void>;
    private writeRecordSync;
}
export declare class WalReplayer {
    private readonly dbPath;
    constructor(dbPath: string);
    replay(knownTxIds?: Set<string>): Promise<{
        addFacts: FactInput[];
        deleteFacts: FactInput[];
        nodeProps: Array<{
            nodeId: number;
            value: unknown;
        }>;
        edgeProps: Array<{
            ids: {
                subjectId: number;
                predicateId: number;
                objectId: number;
            };
            value: unknown;
        }>;
        safeOffset: number;
        version: number;
        committedTx: Array<{
            id: string;
            sessionId?: string;
        }>;
    }>;
}
//# sourceMappingURL=wal.d.ts.map