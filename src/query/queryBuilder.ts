import { FactInput, FactRecord } from '../storage/persistentStore';
import { PersistentStore } from '../storage/persistentStore';

export type FactCriteria = Partial<FactInput>;

export type FrontierOrientation = 'subject' | 'object' | 'both';

interface QueryContext {
  facts: FactRecord[];
  frontier: Set<number>;
  orientation: FrontierOrientation;
}

const EMPTY_CONTEXT: QueryContext = {
  facts: [],
  frontier: new Set<number>(),
  orientation: 'object',
};

export class QueryBuilder {
  private readonly facts: FactRecord[];
  private readonly frontier: Set<number>;
  private readonly orientation: FrontierOrientation;
  private readonly pinnedEpoch?: number;

  constructor(
    private readonly store: PersistentStore,
    context: QueryContext,
    pinnedEpoch?: number,
  ) {
    this.facts = context.facts;
    this.frontier = context.frontier;
    this.orientation = context.orientation;
    this.pinnedEpoch = pinnedEpoch;
  }

  all(): FactRecord[] {
    this.pin();
    try {
      return [...this.facts];
    } finally {
      this.unpin();
    }
  }

  where(predicate: (record: FactRecord) => boolean): QueryBuilder {
    const nextFacts = this.facts.filter((f) => {
      try {
        return Boolean(predicate(f));
      } catch {
        return false;
      }
    });
    const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
    return new QueryBuilder(this.store, {
      facts: nextFacts,
      frontier: nextFrontier,
      orientation: this.orientation,
    });
  }

  limit(n: number): QueryBuilder {
    if (n < 0 || Number.isNaN(n)) {
      return this;
    }
    const nextFacts = this.facts.slice(0, n);
    const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
    return new QueryBuilder(this.store, {
      facts: nextFacts,
      frontier: nextFrontier,
      orientation: this.orientation,
    });
  }

  anchor(orientation: FrontierOrientation): QueryBuilder {
    const nextFrontier = buildInitialFrontier(this.facts, orientation);
    return new QueryBuilder(this.store, {
      facts: [...this.facts],
      frontier: nextFrontier,
      orientation,
    });
  }

  follow(predicate: string): QueryBuilder {
    return this.traverse(predicate, 'forward');
  }

  followReverse(predicate: string): QueryBuilder {
    return this.traverse(predicate, 'reverse');
  }

  private traverse(predicate: string, direction: 'forward' | 'reverse'): QueryBuilder {
    if (this.frontier.size === 0) {
      return new QueryBuilder(this.store, EMPTY_CONTEXT);
    }

    const predicateId = this.store.getNodeIdByValue(predicate);
    if (predicateId === undefined) {
      return new QueryBuilder(this.store, EMPTY_CONTEXT);
    }

    const triples = new Map<string, FactRecord>();

    for (const nodeId of this.frontier.values()) {
      const criteria =
        direction === 'forward'
          ? { subjectId: nodeId, predicateId }
          : { predicateId, objectId: nodeId };

      const matches = this.store.query(criteria);
      const records = this.store.resolveRecords(matches);
      records.forEach((record) => {
        triples.set(encodeTripleKey(record), record);
      });
    }

    const nextFacts = [...triples.values()];
    const nextFrontier = new Set<number>();

    nextFacts.forEach((fact) => {
      if (direction === 'forward') {
        nextFrontier.add(fact.objectId);
      } else {
        nextFrontier.add(fact.subjectId);
      }
    });

    return new QueryBuilder(this.store, {
      facts: nextFacts,
      frontier: nextFrontier,
      orientation: direction === 'forward' ? 'object' : 'subject',
    }, this.pinnedEpoch);
  }

  static fromFindResult(store: PersistentStore, context: QueryContext, pinnedEpoch?: number): QueryBuilder {
    return new QueryBuilder(store, context, pinnedEpoch);
  }

  static empty(store: PersistentStore): QueryBuilder {
    return new QueryBuilder(store, EMPTY_CONTEXT);
  }

  private pin(): void {
    if (this.pinnedEpoch !== undefined) {
      try {
        (this.store as unknown as { pushPinnedEpoch: (e: number) => void }).pushPinnedEpoch(
          this.pinnedEpoch,
        );
      } catch {}
    }
  }

  private unpin(): void {
    if (this.pinnedEpoch !== undefined) {
      try {
        (this.store as unknown as { popPinnedEpoch: () => void }).popPinnedEpoch();
      } catch {}
    }
  }
}

export function buildFindContext(
  store: PersistentStore,
  criteria: FactCriteria,
  anchor: FrontierOrientation,
): QueryContext {
  const query = convertCriteriaToIds(store, criteria);
  if (query === null) {
    return EMPTY_CONTEXT;
  }

  const matches = store.query(query);
  if (matches.length === 0) {
    return EMPTY_CONTEXT;
  }

  const facts = store.resolveRecords(matches);
  const frontier = buildInitialFrontier(facts, anchor);

  return {
    facts,
    frontier,
    orientation: anchor,
  };
}

type IdCriteria = Partial<Record<'subjectId' | 'predicateId' | 'objectId', number>>;

function convertCriteriaToIds(store: PersistentStore, criteria: FactCriteria): IdCriteria | null {
  const result: IdCriteria = {};

  if (criteria.subject !== undefined) {
    const id = store.getNodeIdByValue(criteria.subject);
    if (id === undefined) {
      return null;
    }
    result.subjectId = id;
  }

  if (criteria.predicate !== undefined) {
    const id = store.getNodeIdByValue(criteria.predicate);
    if (id === undefined) {
      return null;
    }
    result.predicateId = id;
  }

  if (criteria.object !== undefined) {
    const id = store.getNodeIdByValue(criteria.object);
    if (id === undefined) {
      return null;
    }
    result.objectId = id;
  }

  return result;
}

function buildInitialFrontier(facts: FactRecord[], anchor: FrontierOrientation): Set<number> {
  const nodes = new Set<number>();
  facts.forEach((fact) => {
    if (anchor === 'subject') {
      nodes.add(fact.subjectId);
      return;
    }
    if (anchor === 'object') {
      nodes.add(fact.objectId);
      return;
    }
    nodes.add(fact.subjectId);
    nodes.add(fact.objectId);
  });
  return nodes;
}

function rebuildFrontier(facts: FactRecord[], orientation: FrontierOrientation): Set<number> {
  if (facts.length === 0) return new Set<number>();
  if (orientation === 'subject') return new Set<number>(facts.map((f) => f.subjectId));
  if (orientation === 'object') return new Set<number>(facts.map((f) => f.objectId));
  const set = new Set<number>();
  facts.forEach((f) => {
    set.add(f.subjectId);
    set.add(f.objectId);
  });
  return set;
}

function encodeTripleKey(fact: FactRecord): string {
  return `${fact.subjectId}:${fact.predicateId}:${fact.objectId}`;
}
