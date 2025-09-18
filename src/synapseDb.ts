import { PersistentStore, FactInput, FactRecord } from './storage/persistentStore';
import { TripleKey } from './storage/propertyStore';
import {
  FactCriteria,
  FrontierOrientation,
  QueryBuilder,
  buildFindContext,
} from './query/queryBuilder';

export interface FactOptions {
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
}

export class SynapseDB {
  private constructor(private readonly store: PersistentStore) {}

  static async open(
    path: string,
    options?: {
      indexDirectory?: string;
      pageSize?: number;
      rebuildIndexes?: boolean;
      compression?: { codec: 'none' | 'brotli'; level?: number };
    },
  ): Promise<SynapseDB> {
    const store = await PersistentStore.open(path, options ?? {});
    return new SynapseDB(store);
  }

  addFact(fact: FactInput, options: FactOptions = {}): FactRecord {
    const persisted = this.store.addFact(fact);

    if (options.subjectProperties) {
      this.store.setNodeProperties(persisted.subjectId, options.subjectProperties);
    }

    if (options.objectProperties) {
      this.store.setNodeProperties(persisted.objectId, options.objectProperties);
    }

    if (options.edgeProperties) {
      const tripleKey: TripleKey = {
        subjectId: persisted.subjectId,
        predicateId: persisted.predicateId,
        objectId: persisted.objectId,
      };
      this.store.setEdgeProperties(tripleKey, options.edgeProperties);
    }

    return {
      ...persisted,
      subjectProperties: this.store.getNodeProperties(persisted.subjectId),
      objectProperties: this.store.getNodeProperties(persisted.objectId),
      edgeProperties: this.store.getEdgeProperties({
        subjectId: persisted.subjectId,
        predicateId: persisted.predicateId,
        objectId: persisted.objectId,
      }),
    };
  }

  listFacts(): FactRecord[] {
    return this.store.listFacts();
  }

  getNodeId(value: string): number | undefined {
    return this.store.getNodeIdByValue(value);
  }

  getNodeValue(id: number): string | undefined {
    return this.store.getNodeValueById(id);
  }

  getNodeProperties(nodeId: number): Record<string, unknown> | undefined {
    return this.store.getNodeProperties(nodeId);
  }

  getEdgeProperties(key: TripleKey): Record<string, unknown> | undefined {
    return this.store.getEdgeProperties(key);
  }

  async flush(): Promise<void> {
    await this.store.flush();
  }

  find(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder {
    const anchor = options?.anchor ?? inferAnchor(criteria);
    const pinned = (this.store as unknown as { getCurrentEpoch: () => number }).getCurrentEpoch?.()
      ?? 0;
    // 对初始 find 也进行临时 pinned 保障
    try {
      (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch?.(pinned);
      const context = buildFindContext(this.store, criteria, anchor);
      return QueryBuilder.fromFindResult(this.store, context, pinned);
    } finally {
      (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch?.();
    }
  }

  deleteFact(fact: FactInput): void {
    this.store.deleteFact(fact);
  }

  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    this.store.setNodeProperties(nodeId, properties);
  }

  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    this.store.setEdgeProperties(key, properties);
  }

  // 事务批次控制（可选）：允许将多次写入合并为一次提交
  beginBatch(): void {
    this.store.beginBatch();
  }

  commitBatch(): void {
    this.store.commitBatch();
  }

  abortBatch(): void {
    this.store.abortBatch();
  }

  async close(): Promise<void> {
    await this.store.close();
  }
}

export type { FactInput, FactRecord };

function inferAnchor(criteria: FactCriteria): FrontierOrientation {
  const hasSubject = criteria.subject !== undefined;
  const hasObject = criteria.object !== undefined;

  if (hasSubject && hasObject) {
    return 'both';
  }
  if (hasSubject) {
    return 'subject';
  }
  return 'object';
}
