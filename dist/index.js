const DEFAULT_PORTS = {
    postgresql: 5432,
    mysql: 3306,
    mariadb: 3306,
    sqlserver: 1433,
};
export function ensureConnectionOptions(options) {
    const missing = [];
    if (!options.driver)
        missing.push('driver');
    if (!options.host)
        missing.push('host');
    if (!options.username)
        missing.push('username');
    if (!options.password)
        missing.push('password');
    if (missing.length > 0) {
        throw new Error(`缺少必要连接字段: ${missing.join(', ')}`);
    }
    return {
        ...options,
        port: options.port ?? DEFAULT_PORTS[options.driver],
    };
}
export function buildConnectionUri(options) {
    const normalized = ensureConnectionOptions(options);
    const credentials = encodeURIComponent(normalized.username);
    const secret = encodeURIComponent(normalized.password);
    const hostSegment = `${normalized.host}:${normalized.port}`;
    const base = `${normalized.driver}://${credentials}:${secret}@${hostSegment}`;
    const databaseSegment = normalized.database ? `/${encodeURIComponent(normalized.database)}` : '';
    const querySegment = buildQueryString(normalized.parameters ?? {});
    return `${base}${databaseSegment}${querySegment}`;
}
export function sanitizeConnectionOptions(options) {
    const normalized = ensureConnectionOptions(options);
    const maskedPassword = normalized.password.replace(/.(?=.{4})/g, '*');
    return {
        ...normalized,
        password: maskedPassword,
    };
}
function buildQueryString(parameters) {
    const entries = Object.entries(parameters);
    if (entries.length === 0) {
        return '';
    }
    const query = entries
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`)
        .join('&');
    return `?${query}`;
}
export { PersistentStore } from './storage/persistentStore.js';
export { SynapseDB } from './synapseDb.js';
export { QueryBuilder } from './query/queryBuilder.js';
//# sourceMappingURL=index.js.map