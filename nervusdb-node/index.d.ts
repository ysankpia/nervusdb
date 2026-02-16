export type ScalarValue = null | boolean | number | string

export type ErrorCategory = 'syntax' | 'execution' | 'storage' | 'compatibility'

export interface NervusErrorPayload {
  code: string
  category: ErrorCategory
  message: string
}

export interface NodeValue {
  type: 'node'
  id: number
  labels: string[]
  properties: Record<string, unknown>
}

export interface RelationshipValue {
  type: 'relationship'
  src: number
  dst: number
  rel_type: string
  properties: Record<string, unknown>
}

export interface PathValue {
  type: 'path' | 'path_legacy'
  nodes: unknown[]
  relationships?: unknown[]
  edges?: unknown[]
}

export type QueryValue = ScalarValue | NodeValue | RelationshipValue | PathValue | Record<string, unknown> | QueryValue[]

export type QueryRow = Record<string, QueryValue>

export class Db {
  static open(path: string): Db
  query(cypher: string): QueryRow[]
  executeWrite(cypher: string): number
  beginWrite(): WriteTxn
  close(): void
}

export class WriteTxn {
  query(cypher: string): void
  commit(): number
  rollback(): void
}
