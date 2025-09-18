import { FactInput, FactRecord } from '../storage/persistentStore.js';
import { PersistentStore } from '../storage/persistentStore.js';
export type FactCriteria = Partial<FactInput>;
export type FrontierOrientation = 'subject' | 'object' | 'both';
interface QueryContext {
    facts: FactRecord[];
    frontier: Set<number>;
    orientation: FrontierOrientation;
}
export declare class QueryBuilder {
    private readonly store;
    private readonly facts;
    private readonly frontier;
    private readonly orientation;
    private readonly pinnedEpoch?;
    constructor(store: PersistentStore, context: QueryContext, pinnedEpoch?: number);
    all(): FactRecord[];
    where(predicate: (record: FactRecord) => boolean): QueryBuilder;
    limit(n: number): QueryBuilder;
    anchor(orientation: FrontierOrientation): QueryBuilder;
    follow(predicate: string): QueryBuilder;
    followReverse(predicate: string): QueryBuilder;
    private traverse;
    static fromFindResult(store: PersistentStore, context: QueryContext, pinnedEpoch?: number): QueryBuilder;
    static empty(store: PersistentStore): QueryBuilder;
    private pin;
    private unpin;
}
export declare function buildFindContext(store: PersistentStore, criteria: FactCriteria, anchor: FrontierOrientation): QueryContext;
export {};
//# sourceMappingURL=queryBuilder.d.ts.map