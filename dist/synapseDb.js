import { PersistentStore } from './storage/persistentStore';
import { QueryBuilder, buildFindContext, } from './query/queryBuilder';
/**
 * SynapseDB - 嵌入式三元组知识库
 *
 * 基于 TypeScript 实现的类 SQLite 单文件数据库，专门用于存储和查询 SPO 三元组数据。
 * 支持分页索引、WAL 事务、快照一致性、自动压缩和垃圾回收。
 *
 * @example
 * ```typescript
 * const db = await SynapseDB.open('/path/to/database.synapsedb', {
 *   pageSize: 2000,
 *   enableLock: true,
 *   compression: { codec: 'brotli', level: 6 }
 * });
 *
 * db.addFact({ subject: 'Alice', predicate: 'knows', object: 'Bob' });
 * await db.flush();
 *
 * const results = db.find({ predicate: 'knows' }).all();
 * await db.close();
 * ```
 */
export class SynapseDB {
    store;
    constructor(store) {
        this.store = store;
    }
    /**
     * 打开或创建 SynapseDB 数据库
     *
     * @param path 数据库文件路径，如果不存在将自动创建
     * @param options 数据库配置选项
     * @returns Promise<SynapseDB> 数据库实例
     *
     * @example
     * ```typescript
     * // 基本用法
     * const db = await SynapseDB.open('./my-database.synapsedb');
     *
     * // 带配置的用法
     * const db = await SynapseDB.open('./my-database.synapsedb', {
     *   pageSize: 1500,
     *   enableLock: true,
     *   registerReader: true,
     *   compression: { codec: 'brotli', level: 4 }
     * });
     * ```
     *
     * @throws {Error} 当文件无法访问或锁定冲突时
     */
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
        const v = this.store.getNodeProperties(nodeId);
        // 对外 API 约定：未设置返回 null，便于测试与调用方判空
        return v ?? null;
    }
    getEdgeProperties(key) {
        const v = this.store.getEdgeProperties(key);
        return v ?? null;
    }
    async flush() {
        await this.store.flush();
    }
    find(criteria, options) {
        const anchor = options?.anchor ?? inferAnchor(criteria);
        const pinned = this.store.getCurrentEpoch?.() ?? 0;
        // 对初始 find 也进行临时 pinned 保障
        try {
            this.store.pushPinnedEpoch?.(pinned);
            const context = buildFindContext(this.store, criteria, anchor);
            return QueryBuilder.fromFindResult(this.store, context, pinned);
        }
        finally {
            this.store.popPinnedEpoch?.();
        }
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
    beginBatch(options) {
        this.store.beginBatch(options);
    }
    commitBatch(options) {
        this.store.commitBatch(options);
    }
    abortBatch() {
        this.store.abortBatch();
    }
    async close() {
        await this.store.close();
    }
    // 读快照：在给定回调期间固定当前 epoch，避免 mid-chain 刷新 readers 造成视图漂移
    async withSnapshot(fn) {
        const epoch = this.store.getCurrentEpoch?.() ?? 0;
        try {
            // 等待读者注册完成，确保快照安全
            await this.store.pushPinnedEpoch?.(epoch);
            return await fn(this);
        }
        finally {
            await this.store.popPinnedEpoch?.();
        }
    }
    // 暂存层指标（实验性）：仅用于观测与基准
    getStagingMetrics() {
        return (this.store.getStagingMetrics?.() ?? { lsmMemtable: 0 });
    }
}
function inferAnchor(criteria) {
    const hasSubject = criteria.subject !== undefined;
    const hasObject = criteria.object !== undefined;
    const hasPredicate = criteria.predicate !== undefined;
    if (hasSubject && hasObject) {
        return 'both';
    }
    if (hasSubject) {
        return 'subject';
    }
    // p+o 查询通常希望锚定主语集合，便于后续正向联想
    if (hasObject && hasPredicate) {
        return 'subject';
    }
    // 仅 object 的场景保持锚定到宾语，便于 reverse follow（测试依赖）
    if (hasObject) {
        return 'object';
    }
    return 'object';
}
//# sourceMappingURL=synapseDb.js.map