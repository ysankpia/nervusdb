const EMPTY_CONTEXT = {
    facts: [],
    frontier: new Set(),
    orientation: 'object',
};
export class QueryBuilder {
    store;
    facts;
    frontier;
    orientation;
    constructor(store, context) {
        this.store = store;
        this.facts = context.facts;
        this.frontier = context.frontier;
        this.orientation = context.orientation;
    }
    all() {
        return [...this.facts];
    }
    where(predicate) {
        const nextFacts = this.facts.filter((f) => {
            try {
                return Boolean(predicate(f));
            }
            catch {
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
    limit(n) {
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
    anchor(orientation) {
        const nextFrontier = buildInitialFrontier(this.facts, orientation);
        return new QueryBuilder(this.store, {
            facts: [...this.facts],
            frontier: nextFrontier,
            orientation,
        });
    }
    follow(predicate) {
        return this.traverse(predicate, 'forward');
    }
    followReverse(predicate) {
        return this.traverse(predicate, 'reverse');
    }
    traverse(predicate, direction) {
        if (this.frontier.size === 0) {
            return new QueryBuilder(this.store, EMPTY_CONTEXT);
        }
        const predicateId = this.store.getNodeIdByValue(predicate);
        if (predicateId === undefined) {
            return new QueryBuilder(this.store, EMPTY_CONTEXT);
        }
        const triples = new Map();
        for (const nodeId of this.frontier.values()) {
            const criteria = direction === 'forward'
                ? { subjectId: nodeId, predicateId }
                : { predicateId, objectId: nodeId };
            const matches = this.store.query(criteria);
            const records = this.store.resolveRecords(matches);
            records.forEach((record) => {
                triples.set(encodeTripleKey(record), record);
            });
        }
        const nextFacts = [...triples.values()];
        const nextFrontier = new Set();
        nextFacts.forEach((fact) => {
            if (direction === 'forward') {
                nextFrontier.add(fact.objectId);
            }
            else {
                nextFrontier.add(fact.subjectId);
            }
        });
        return new QueryBuilder(this.store, {
            facts: nextFacts,
            frontier: nextFrontier,
            orientation: direction === 'forward' ? 'object' : 'subject',
        });
    }
    static fromFindResult(store, context) {
        return new QueryBuilder(store, context);
    }
    static empty(store) {
        return new QueryBuilder(store, EMPTY_CONTEXT);
    }
}
export function buildFindContext(store, criteria, anchor) {
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
function convertCriteriaToIds(store, criteria) {
    const result = {};
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
function buildInitialFrontier(facts, anchor) {
    const nodes = new Set();
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
function rebuildFrontier(facts, orientation) {
    if (facts.length === 0)
        return new Set();
    if (orientation === 'subject')
        return new Set(facts.map((f) => f.subjectId));
    if (orientation === 'object')
        return new Set(facts.map((f) => f.objectId));
    const set = new Set();
    facts.forEach((f) => {
        set.add(f.subjectId);
        set.add(f.objectId);
    });
    return set;
}
function encodeTripleKey(fact) {
    return `${fact.subjectId}:${fact.predicateId}:${fact.objectId}`;
}
//# sourceMappingURL=queryBuilder.js.map