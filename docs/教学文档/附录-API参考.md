# 附录 · API 参考（摘要）

> 完整类型以源码为准（`src/`），此处汇总主要接口与参数说明，便于查阅。

## SynapseDB

```ts
class SynapseDB {
  static open(path: string, options?: SynapseDBOpenOptions): Promise<SynapseDB>;
  addFact(fact: FactInput, opts?: {
    subjectProperties?: Record<string, unknown>;
    objectProperties?: Record<string, unknown>;
    edgeProperties?: Record<string, unknown>;
  }): FactRecord;
  listFacts(): FactRecord[];
  streamFacts(criteria?: Partial<{subject:string;predicate:string;object:string}>, batchSize?: number): AsyncGenerator<FactRecord[],void,unknown>;
  find(criteria: FactCriteria, options?: { anchor?: FrontierOrientation }): QueryBuilder;
  deleteFact(fact: FactInput): void;
  setNodeProperties(nodeId: number, props: Record<string, unknown>): void;
  setEdgeProperties(key: TripleKey, props: Record<string, unknown>): void;
  getNodeProperties(nodeId: number): Record<string, unknown> | null;
  getEdgeProperties(key: TripleKey): Record<string, unknown> | null;
  beginBatch(options?: BeginBatchOptions): void;
  commitBatch(options?: CommitBatchOptions): void;
  abortBatch(): void;
  flush(): Promise<void>;
  withSnapshot<T>(fn: (db: SynapseDB) => Promise<T> | T): Promise<T>;
  close(): Promise<void>;
}
```

## 关键类型

```ts
type FactInput = { subject: string; predicate: string; object: string };
type FactRecord = FactInput & {
  subjectId: number; predicateId: number; objectId: number;
  subjectProperties?: Record<string, unknown>;
  objectProperties?: Record<string, unknown>;
  edgeProperties?: Record<string, unknown>;
};

type FactCriteria = Partial<FactInput>;
type FrontierOrientation = 'subject' | 'object' | 'both';

type TripleKey = { subjectId: number; predicateId: number; objectId: number };

interface BeginBatchOptions { txId?: string; sessionId?: string }
interface CommitBatchOptions { durable?: boolean }
```

## 打开选项（SynapseDBOpenOptions）

```ts
interface SynapseDBOpenOptions {
  indexDirectory?: string;            // 默认 <db>.pages
  pageSize?: number;                  // 建议 1K~2K
  rebuildIndexes?: boolean;           // 强制重建索引
  compression?: { codec: 'none'|'brotli'; level?: number };
  enableLock?: boolean;               // 生产建议开启
  registerReader?: boolean;           // 默认 true
  stagingMode?: 'default'|'lsm-lite'; // 实验
  enablePersistentTxDedupe?: boolean; // 幂等
  maxRememberTxIds?: number;          // 默认 1000
}
```

## QueryBuilder（链式联想）

```ts
db.find({ subject: 'S' })
  .follow('R')
  .followReverse('R2')
  .where(edge => (edge.edgeProperties?.weight as number) > 10)
  .limit(100)
  .all();
```

