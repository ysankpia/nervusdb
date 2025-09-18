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
export { PersistentStore } from './storage/persistentStore.js';
export type { FactInput, PersistedFact } from './storage/persistentStore.js';
export { SynapseDB } from './synapseDb.js';
export type { FactRecord } from './synapseDb.js';
export { QueryBuilder } from './query/queryBuilder.js';
export type { FactCriteria, FrontierOrientation } from './query/queryBuilder.js';
//# sourceMappingURL=index.d.ts.map