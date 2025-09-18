export type IndexOrder = 'SPO' | 'POS' | 'OSP' | 'SOP' | 'PSO' | 'OPS';

export interface OrderedTriple {
  subjectId: number;
  predicateId: number;
  objectId: number;
}

interface IndexDescriptor {
  order: IndexOrder;
  projection: Array<keyof OrderedTriple>;
  primary: keyof OrderedTriple;
}

const INDEX_DESCRIPTORS: IndexDescriptor[] = [
  { order: 'SPO', projection: ['subjectId', 'predicateId', 'objectId'], primary: 'subjectId' },
  { order: 'SOP', projection: ['subjectId', 'objectId', 'predicateId'], primary: 'subjectId' },
  { order: 'POS', projection: ['predicateId', 'objectId', 'subjectId'], primary: 'predicateId' },
  { order: 'PSO', projection: ['predicateId', 'subjectId', 'objectId'], primary: 'predicateId' },
  { order: 'OSP', projection: ['objectId', 'subjectId', 'predicateId'], primary: 'objectId' },
  { order: 'OPS', projection: ['objectId', 'predicateId', 'subjectId'], primary: 'objectId' },
];

const ORDER_TO_DESCRIPTOR = new Map<IndexOrder, IndexDescriptor>(
  INDEX_DESCRIPTORS.map((descriptor) => [descriptor.order, descriptor]),
);

export class TripleIndexes {
  // 仅存储“增量暂存”的索引（flush 后清空）
  private readonly indexes = new Map<IndexOrder, Map<number, OrderedTriple[]>>();

  constructor(initialData?: Map<IndexOrder, OrderedTriple[]>) {
    INDEX_DESCRIPTORS.forEach(({ order }) => {
      const seed = initialData?.get(order) ?? [];
      const buckets = new Map<number, OrderedTriple[]>();
      seed.forEach((triple) => {
        this.insertIntoBuckets(buckets, triple, order);
      });
      this.indexes.set(order, buckets);
    });
  }

  seed(triples: OrderedTriple[]): void {
    INDEX_DESCRIPTORS.forEach(({ order }) => {
      const buckets = this.indexes.get(order);
      if (!buckets) {
        return;
      }
      buckets.clear();
      triples.forEach((triple) => {
        this.insertIntoBuckets(buckets, triple, order);
      });
    });
  }

  add(triple: OrderedTriple): void {
    INDEX_DESCRIPTORS.forEach(({ order }) => {
      const buckets = this.indexes.get(order);
      if (!buckets) {
        return;
      }
      this.insertIntoBuckets(buckets, triple, order);
    });
  }

  get(order: IndexOrder): OrderedTriple[] {
    const buckets = this.indexes.get(order);
    if (!buckets) {
      return [];
    }
    const aggregated: OrderedTriple[] = [];
    for (const bucket of buckets.values()) {
      aggregated.push(...bucket);
    }
    const descriptor = ORDER_TO_DESCRIPTOR.get(order);
    if (!descriptor) {
      return aggregated;
    }
    return [...aggregated].sort((a, b) => compareTriples(a, b, descriptor));
  }

  query(criteria: Partial<OrderedTriple>): OrderedTriple[] {
    const order = getBestIndexKey(criteria);
    const descriptor = ORDER_TO_DESCRIPTOR.get(order);
    if (!descriptor) {
      return [];
    }

    const buckets = this.indexes.get(order);
    if (!buckets) {
      return [];
    }

    const primaryValue = criteria[descriptor.primary];
    const candidates: OrderedTriple[] = [];

    if (primaryValue !== undefined) {
      const bucket = buckets.get(primaryValue);
      if (!bucket) {
        return [];
      }
      candidates.push(...bucket);
    } else {
      for (const bucket of buckets.values()) {
        candidates.push(...bucket);
      }
    }

    if (candidates.length === 0) {
      return [];
    }

    return filterBucket(candidates, criteria, descriptor);
  }

  serialize(): Buffer {
    // 仅序列化“暂存”索引，便于在测试或断点恢复阶段保留未落盘增量
    const buffers: Buffer[] = [];
    const orderCount = Buffer.allocUnsafe(4);
    orderCount.writeUInt32LE(INDEX_DESCRIPTORS.length, 0);
    buffers.push(orderCount);

    for (const descriptor of INDEX_DESCRIPTORS) {
      const { order } = descriptor;
      const staged = this.get(order);
      const orderMarker = Buffer.from(order, 'utf8');
      const marker = Buffer.alloc(4, 0);
      orderMarker.copy(marker, 0);
      buffers.push(marker);

      const countBuffer = Buffer.allocUnsafe(4);
      countBuffer.writeUInt32LE(staged.length, 0);
      buffers.push(countBuffer);

      if (staged.length === 0) continue;

      const body = Buffer.allocUnsafe(staged.length * 12);
      staged.forEach((triple, index) => {
        const offset = index * 12;
        body.writeUInt32LE(triple.subjectId, offset);
        body.writeUInt32LE(triple.predicateId, offset + 4);
        body.writeUInt32LE(triple.objectId, offset + 8);
      });
      buffers.push(body);
    }

    return Buffer.concat(buffers);
  }

  static deserialize(buffer: Buffer): TripleIndexes {
    if (buffer.length === 0) return new TripleIndexes();

    let offset = 0;
    const readUInt32 = (): number => {
      const value = buffer.readUInt32LE(offset);
      offset += 4;
      return value;
    };

    const indexCount = readUInt32();
    const staged = new Map<IndexOrder, OrderedTriple[]>();

    for (let i = 0; i < indexCount; i += 1) {
      const marker = buffer
        .subarray(offset, offset + 4)
        .toString('utf8')
        .replace(/\0+$/, '') as IndexOrder;
      offset += 4;
      const tripleCount = readUInt32();
      if (marker !== 'SPO') {
        // 跳过非 SPO 顺序的重复数据
        offset += tripleCount * 12;
        continue;
      }
      const triples: OrderedTriple[] = [];
      for (let j = 0; j < tripleCount; j += 1) {
        const subjectId = readUInt32();
        const predicateId = readUInt32();
        const objectId = readUInt32();
        triples.push({ subjectId, predicateId, objectId });
      }
      staged.set(marker, triples);
    }

    const indexes = new TripleIndexes();
    // 将暂存三元组回填到 staging 结构
    for (const [, list] of staged.entries()) {
      list.forEach((t) => indexes.add(t));
    }
    return indexes;
  }

  private insertIntoBuckets(
    buckets: Map<number, OrderedTriple[]>,
    triple: OrderedTriple,
    order: IndexOrder,
  ): void {
    const descriptor = ORDER_TO_DESCRIPTOR.get(order);
    if (!descriptor) {
      return;
    }

    const primaryValue = triple[descriptor.primary];
    const bucket = buckets.get(primaryValue) ?? [];
    if (!buckets.has(primaryValue)) {
      buckets.set(primaryValue, bucket);
    }

    const clone = { ...triple };
    const index = binarySearchInsertPosition(bucket, clone, descriptor);
    bucket.splice(index, 0, clone);
  }
}

export function getBestIndexKey(criteria: Partial<OrderedTriple>): IndexOrder {
  const hasS = criteria.subjectId !== undefined;
  const hasP = criteria.predicateId !== undefined;
  const hasO = criteria.objectId !== undefined;

  // 优先选择能覆盖前缀最多的顺序
  if (hasS && hasP) return 'SPO';
  if (hasS && hasO) return 'SOP';
  if (hasP && hasO) return 'POS';
  if (hasS) return 'SPO';
  if (hasP) return 'POS';
  if (hasO) return 'OSP';
  return 'SPO';
}

function matchesCriteria(triple: OrderedTriple, criteria: Partial<OrderedTriple>): boolean {
  if (criteria.subjectId !== undefined && triple.subjectId !== criteria.subjectId) {
    return false;
  }
  if (criteria.predicateId !== undefined && triple.predicateId !== criteria.predicateId) {
    return false;
  }
  if (criteria.objectId !== undefined && triple.objectId !== criteria.objectId) {
    return false;
  }
  return true;
}

function binarySearchInsertPosition(
  bucket: OrderedTriple[],
  candidate: OrderedTriple,
  descriptor: IndexDescriptor,
): number {
  let low = 0;
  let high = bucket.length;

  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    const compareResult = compareTriples(bucket[mid], candidate, descriptor);
    if (compareResult <= 0) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low;
}

function compareTriples(a: OrderedTriple, b: OrderedTriple, descriptor: IndexDescriptor): number {
  const [primary, ...rest] = descriptor.projection;
  const primaryDelta = a[primary] - b[primary];
  if (primaryDelta !== 0) {
    return primaryDelta;
  }

  for (const key of rest) {
    const delta = a[key] - b[key];
    if (delta !== 0) {
      return delta;
    }
  }

  return 0;
}

function filterBucket(
  bucket: OrderedTriple[],
  criteria: Partial<OrderedTriple>,
  descriptor: IndexDescriptor,
): OrderedTriple[] {
  const { primary, projection } = descriptor;
  const [, ...rest] = projection;

  if (criteria[primary] === undefined || rest.every((key) => criteria[key] === undefined)) {
    return bucket.filter((triple) => matchesCriteria(triple, criteria));
  }

  const lowerBound = lowerBoundIndex(bucket, criteria, descriptor);
  const upperBound = upperBoundIndex(bucket, criteria, descriptor);

  const results: OrderedTriple[] = [];
  for (let i = lowerBound; i < upperBound; i += 1) {
    const triple = bucket[i];
    if (matchesCriteria(triple, criteria)) {
      results.push(triple);
    }
  }
  return results;
}

function lowerBoundIndex(
  bucket: OrderedTriple[],
  criteria: Partial<OrderedTriple>,
  descriptor: IndexDescriptor,
): number {
  let low = 0;
  let high = bucket.length;

  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    if (compareTripleWithCriteria(bucket[mid], criteria, descriptor) < 0) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low;
}

function upperBoundIndex(
  bucket: OrderedTriple[],
  criteria: Partial<OrderedTriple>,
  descriptor: IndexDescriptor,
): number {
  let low = 0;
  let high = bucket.length;

  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    if (compareTripleWithCriteria(bucket[mid], criteria, descriptor) <= 0) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low;
}

function compareTripleWithCriteria(
  triple: OrderedTriple,
  criteria: Partial<OrderedTriple>,
  descriptor: IndexDescriptor,
): number {
  const { projection } = descriptor;
  for (const key of projection) {
    const value = criteria[key];
    if (value === undefined) {
      continue;
    }
    const delta = triple[key] - value;
    if (delta !== 0) {
      return delta;
    }
  }
  return 0;
}
