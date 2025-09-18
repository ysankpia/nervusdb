export type SupportedDriver = 'postgresql' | 'mysql' | 'mariadb' | 'sqlserver';
export interface ConnectionOptions {
    driver: SupportedDriver;
    host: string;
    username: string;
    password: string;
    database?: string;
    port?: number;
    parameters?: Record<string, string | number | boolean>;
}
export interface SanitizedConnectionOptions extends Omit<ConnectionOptions, 'password'> {
    password: string;
}
export declare function ensureConnectionOptions(options: ConnectionOptions): ConnectionOptions;
export declare function buildConnectionUri(options: ConnectionOptions): string;
export declare function sanitizeConnectionOptions(options: ConnectionOptions): SanitizedConnectionOptions;
export { PersistentStore } from './storage/persistentStore';
export type { FactInput, PersistedFact } from './storage/persistentStore';
export { SynapseDB } from './synapseDb';
export type { FactRecord } from './synapseDb';
export { QueryBuilder } from './query/queryBuilder';
export type { FactCriteria, FrontierOrientation } from './query/queryBuilder';
//# sourceMappingURL=index.d.ts.map