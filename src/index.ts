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

const DEFAULT_PORTS: Record<SupportedDriver, number> = {
  postgresql: 5432,
  mysql: 3306,
  mariadb: 3306,
  sqlserver: 1433,
};

export interface SanitizedConnectionOptions extends Omit<ConnectionOptions, 'password'> {
  password: string;
}

export function ensureConnectionOptions(options: ConnectionOptions): ConnectionOptions {
  const missing: Array<keyof ConnectionOptions> = [];

  if (!options.driver) missing.push('driver');
  if (!options.host) missing.push('host');
  if (!options.username) missing.push('username');
  if (!options.password) missing.push('password');

  if (missing.length > 0) {
    throw new Error(`缺少必要连接字段: ${missing.join(', ')}`);
  }

  return {
    ...options,
    port: options.port ?? DEFAULT_PORTS[options.driver],
  };
}

export function buildConnectionUri(options: ConnectionOptions): string {
  const normalized = ensureConnectionOptions(options);
  const credentials = encodeURIComponent(normalized.username);
  const secret = encodeURIComponent(normalized.password);
  const hostSegment = `${normalized.host}:${normalized.port}`;

  const base = `${normalized.driver}://${credentials}:${secret}@${hostSegment}`;
  const databaseSegment = normalized.database ? `/${encodeURIComponent(normalized.database)}` : '';
  const querySegment = buildQueryString(normalized.parameters ?? {});

  return `${base}${databaseSegment}${querySegment}`;
}

export function sanitizeConnectionOptions(options: ConnectionOptions): SanitizedConnectionOptions {
  const normalized = ensureConnectionOptions(options);
  const maskedPassword = normalized.password.replace(/.(?=.{4})/g, '*');
  return {
    ...normalized,
    password: maskedPassword,
  };
}

function buildQueryString(parameters: Record<string, string | number | boolean>): string {
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

export { PersistentStore } from './storage/persistentStore';
export type { FactInput, PersistedFact } from './storage/persistentStore';
export { SynapseDB } from './synapseDb';
export type { FactRecord } from './synapseDb';
export { QueryBuilder } from './query/queryBuilder';
export type { FactCriteria, FrontierOrientation } from './query/queryBuilder';
