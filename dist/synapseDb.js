import { PersistentStore } from './storage/persistentStore';
import { QueryBuilder, buildFindContext, } from './query/queryBuilder';
export class SynapseDB {
    store;
    constructor(store) {
        this.store = store;
    }
    static async open(path, options) {
        const store = await PersistentStore.open(path, options ?? {});
        return new SynapseDB(store);
    }
    addFact(fact, options = {}) {
        const persisted = this.store.addFact(fact);
        if (options.subjectProperties) {
            this.store.setNodeProperties(persisted.subjectId, options.subjectProperties);
        }
        if (options.objectProperties) {
            this.store.setNodeProperties(persisted.objectId, options.objectProperties);
        }
        if (options.edgeProperties) {
            const tripleKey = {
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
    listFacts() {
        return this.store.listFacts();
    }
    getNodeId(value) {
        return this.store.getNodeIdByValue(value);
    }
    getNodeValue(id) {
        return this.store.getNodeValueById(id);
    }
    getNodeProperties(nodeId) {
        return this.store.getNodeProperties(nodeId);
    }
    getEdgeProperties(key) {
        return this.store.getEdgeProperties(key);
    }
    async flush() {
        await this.store.flush();
    }
    find(criteria, options) {
        const anchor = options?.anchor ?? inferAnchor(criteria);
        const context = buildFindContext(this.store, criteria, anchor);
        return QueryBuilder.fromFindResult(this.store, context);
    }
    deleteFact(fact) {
        this.store.deleteFact(fact);
    }
    setNodeProperties(nodeId, properties) {
        this.store.setNodeProperties(nodeId, properties);
    }
    setEdgeProperties(key, properties) {
        this.store.setEdgeProperties(key, properties);
    }
    // 事务批次控制（可选）：允许将多次写入合并为一次提交
    beginBatch() {
        this.store.beginBatch();
    }
    commitBatch() {
        this.store.commitBatch();
    }
    abortBatch() {
        this.store.abortBatch();
    }
    async close() {
        await this.store.close();
    }
}
function inferAnchor(criteria) {
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
//# sourceMappingURL=synapseDb.js.map