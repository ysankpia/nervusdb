import { FactInput, FactRecord } from '../storage/persistentStore';
import { PersistentStore } from '../storage/persistentStore';
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
    constructor(store: PersistentStore, context: QueryContext);
    all(): FactRecord[];
    where(predicate: (record: FactRecord) => boolean): QueryBuilder;
    limit(n: number): QueryBuilder;
    anchor(orientation: FrontierOrientation): QueryBuilder;
    follow(predicate: string): QueryBuilder;
    followReverse(predicate: string): QueryBuilder;
    private traverse;
    static fromFindResult(store: PersistentStore, context: QueryContext): QueryBuilder;
    static empty(store: PersistentStore): QueryBuilder;
}
export declare function buildFindContext(store: PersistentStore, criteria: FactCriteria, anchor: FrontierOrientation): QueryContext;
export {};
//# sourceMappingURL=queryBuilder.d.ts.map