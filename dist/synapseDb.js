import { PersistentStore } from './storage/persistentStore.js';
import { QueryBuilder, StreamingQueryBuilder, buildFindContext, buildStreamingFindContext, buildFindContextFromProperty, buildFindContextFromLabel, } from './query/queryBuilder.js';
import { AggregationPipeline } from './query/aggregation.js';
import { VariablePathBuilder } from './query/path/variable.js';
import { PatternBuilder } from './query/pattern/match.js';
import { MinHeap } from './utils/minHeap.js';
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
    // 流式查询：逐批返回事实记录，避免大结果集内存压力
    async *streamFacts(criteria, batchSize = 1000) {
        // 将字符串条件转换为ID条件
        const encodedCriteria = {};
        if (criteria?.subject) {
            const subjectId = this.store.getNodeIdByValue(criteria.subject);
            if (subjectId !== undefined)
                encodedCriteria.subjectId = subjectId;
            else
                return; // 主语不存在，返回空
        }
        if (criteria?.predicate) {
            const predicateId = this.store.getNodeIdByValue(criteria.predicate);
            if (predicateId !== undefined)
                encodedCriteria.predicateId = predicateId;
            else
                return; // 谓语不存在，返回空
        }
        if (criteria?.object) {
            const objectId = this.store.getNodeIdByValue(criteria.object);
            if (objectId !== undefined)
                encodedCriteria.objectId = objectId;
            else
                return; // 宾语不存在，返回空
        }
        // 使用底层流式查询
        for await (const batch of this.store.streamFactRecords(encodedCriteria, batchSize)) {
            if (batch.length > 0) {
                yield batch;
            }
        }
    }
    // 兼容别名：满足测试与直觉 API（与 streamFacts 等价）
    findStream(criteria, options) {
        return this.streamFacts(criteria, options?.batchSize);
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
    /**
     * 流式查询 - 真正内存高效的大数据集查询
     * @param criteria 查询条件
     * @param options 查询选项
     * @returns StreamingQueryBuilder 支持异步迭代，内存占用恒定
     * @example
     * ```typescript
     * // 流式处理大数据集，内存占用恒定
     * for await (const fact of db.findStreaming({ predicate: 'HAS_METHOD' })) {
     *   console.log(fact);
     * }
     * ```
     */
    async findStreaming(criteria, options) {
        const anchor = options?.anchor ?? inferAnchor(criteria);
        const pinned = this.store.getCurrentEpoch?.() ?? 0;
        // 流式查询始终使用快照模式以保证一致性
        try {
            this.store.pushPinnedEpoch?.(pinned);
            const context = await buildStreamingFindContext(this.store, criteria, anchor);
            return new StreamingQueryBuilder(this.store, context, pinned);
        }
        finally {
            this.store.popPinnedEpoch?.();
        }
    }
    find(criteria, options) {
        const anchor = options?.anchor ?? inferAnchor(criteria);
        const pinned = this.store.getCurrentEpoch?.() ?? 0;
        // 检查是否有分页索引数据，如果没有则不使用快照模式
        const pagedReaders = this.store.pagedReaders;
        let hasPagedData = false;
        if (pagedReaders?.size > 0) {
            // 检查索引是否真的包含数据
            const spoReader = pagedReaders.get('SPO');
            if (spoReader) {
                const primaryValues = spoReader.getPrimaryValues?.() ?? [];
                hasPagedData = primaryValues.length > 0;
            }
        }
        if (hasPagedData) {
            // 有分页索引数据时，使用快照模式保证一致性
            try {
                this.store.pushPinnedEpoch?.(pinned);
                const context = buildFindContext(this.store, criteria, anchor);
                return QueryBuilder.fromFindResult(this.store, context, pinned);
            }
            finally {
                this.store.popPinnedEpoch?.();
            }
        }
        else {
            // 没有分页索引数据时，直接使用常规查询（不设置快照）
            const context = buildFindContext(this.store, criteria, anchor);
            return QueryBuilder.fromFindResult(this.store, context);
        }
    }
    /**
     * 基于节点属性进行查询
     * @param propertyFilter 属性过滤条件
     * @param options 查询选项
     * @example
     * ```typescript
     * // 查找所有年龄为25的用户
     * const users = db.findByNodeProperty(
     *   { propertyName: 'age', value: 25 },
     *   { anchor: 'subject' }
     * ).all();
     *
     * // 查找年龄在25-35之间的用户
     * const adults = db.findByNodeProperty({
     *   propertyName: 'age',
     *   range: { min: 25, max: 35, includeMin: true, includeMax: true }
     * }).all();
     * ```
     */
    findByNodeProperty(propertyFilter, options) {
        const anchor = options?.anchor ?? 'subject';
        const pinned = this.store.getCurrentEpoch?.() ?? 0;
        // 检查是否有分页索引数据
        const pagedReaders = this.store.pagedReaders;
        let hasPagedData = false;
        if (pagedReaders?.size > 0) {
            const spoReader = pagedReaders.get('SPO');
            if (spoReader) {
                const primaryValues = spoReader.getPrimaryValues?.() ?? [];
                hasPagedData = primaryValues.length > 0;
            }
        }
        if (hasPagedData) {
            try {
                this.store.pushPinnedEpoch?.(pinned);
                const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'node');
                return QueryBuilder.fromFindResult(this.store, context, pinned);
            }
            finally {
                this.store.popPinnedEpoch?.();
            }
        }
        else {
            const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'node');
            return QueryBuilder.fromFindResult(this.store, context);
        }
    }
    /**
     * 基于边属性进行查询
     * @param propertyFilter 属性过滤条件
     * @param options 查询选项
     * @example
     * ```typescript
     * // 查找所有权重为0.8的关系
     * const strongRelations = db.findByEdgeProperty(
     *   { propertyName: 'weight', value: 0.8 }
     * ).all();
     * ```
     */
    findByEdgeProperty(propertyFilter, options) {
        const anchor = options?.anchor ?? 'subject';
        const pinned = this.store.getCurrentEpoch?.() ?? 0;
        // 检查是否有分页索引数据
        const pagedReaders = this.store.pagedReaders;
        let hasPagedData = false;
        if (pagedReaders?.size > 0) {
            const spoReader = pagedReaders.get('SPO');
            if (spoReader) {
                const primaryValues = spoReader.getPrimaryValues?.() ?? [];
                hasPagedData = primaryValues.length > 0;
            }
        }
        if (hasPagedData) {
            try {
                this.store.pushPinnedEpoch?.(pinned);
                const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'edge');
                return QueryBuilder.fromFindResult(this.store, context, pinned);
            }
            finally {
                this.store.popPinnedEpoch?.();
            }
        }
        else {
            const context = buildFindContextFromProperty(this.store, propertyFilter, anchor, 'edge');
            return QueryBuilder.fromFindResult(this.store, context);
        }
    }
    /**
     * 基于节点标签进行查询
     * @param labels 单个或多个标签
     * @param options 查询选项：{ mode?: 'AND' | 'OR', anchor?: 'subject'|'object'|'both' }
     */
    findByLabel(labels, options) {
        const anchor = options?.anchor ?? 'subject';
        const pinned = this.store.getCurrentEpoch?.() ?? 0;
        // 同 find()/属性查询：如果已有分页索引数据，采用快照模式
        const pagedReaders = this.store.pagedReaders;
        let hasPagedData = false;
        if (pagedReaders?.size > 0) {
            const spoReader = pagedReaders.get('SPO');
            if (spoReader) {
                const primaryValues = spoReader.getPrimaryValues?.() ?? [];
                hasPagedData = primaryValues.length > 0;
            }
        }
        if (hasPagedData) {
            try {
                this.store.pushPinnedEpoch?.(pinned);
                const context = buildFindContextFromLabel(this.store, labels, { mode: options?.mode }, anchor);
                return QueryBuilder.fromFindResult(this.store, context, pinned);
            }
            finally {
                this.store.popPinnedEpoch?.();
            }
        }
        else {
            const context = buildFindContextFromLabel(this.store, labels, { mode: options?.mode }, anchor);
            return QueryBuilder.fromFindResult(this.store, context);
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
    // 聚合入口
    aggregate() {
        return new AggregationPipeline(this.store);
    }
    // 模式匹配入口（最小实现）
    match() {
        return new PatternBuilder(this.store);
    }
    // 最短路径：基于 BFS，返回边序列（不存在则返回 null）
    shortestPath(from, to, options) {
        const startId = this.store.getNodeIdByValue(from);
        const endId = this.store.getNodeIdByValue(to);
        if (startId === undefined || endId === undefined)
            return null;
        const dir = options?.direction ?? 'forward';
        const maxHops = Math.max(1, options?.maxHops ?? 8);
        const predIds = options?.predicates
            ? options.predicates
                .map((p) => this.store.getNodeIdByValue(p))
                .filter((x) => typeof x === 'number')
            : null;
        const qNeighbors = (nid) => {
            const outs = [];
            const pushMatches = (criteria) => {
                const enc = this.store.query(criteria);
                outs.push(...this.store.resolveRecords(enc));
            };
            if (dir === 'forward' || dir === 'both') {
                if (predIds && predIds.length > 0) {
                    for (const pid of predIds)
                        pushMatches({ subjectId: nid, predicateId: pid });
                }
                else {
                    pushMatches({ subjectId: nid });
                }
            }
            if (dir === 'reverse' || dir === 'both') {
                if (predIds && predIds.length > 0) {
                    for (const pid of predIds)
                        pushMatches({ predicateId: pid, objectId: nid });
                }
                else {
                    pushMatches({ objectId: nid });
                }
            }
            return outs;
        };
        const queue = [{ node: startId, path: [] }];
        const visited = new Set([startId]);
        let depth = 0;
        while (queue.length > 0 && depth <= maxHops) {
            const levelSize = queue.length;
            for (let i = 0; i < levelSize; i++) {
                const cur = queue.shift();
                if (cur.node === endId)
                    return cur.path;
                const neigh = qNeighbors(cur.node);
                for (const e of neigh) {
                    const nextNode = e.subjectId === cur.node ? e.objectId : e.subjectId;
                    if (visited.has(nextNode))
                        continue;
                    visited.add(nextNode);
                    queue.push({ node: nextNode, path: [...cur.path, e] });
                }
            }
            depth += 1;
        }
        return null;
    }
    // 双向 BFS 最短路径（无权），对大图更高效 - 优化版本
    shortestPathBidirectional(from, to, options) {
        const startId = this.store.getNodeIdByValue(from);
        const endId = this.store.getNodeIdByValue(to);
        if (startId === undefined || endId === undefined)
            return null;
        if (startId === endId)
            return [];
        const maxHops = Math.max(1, options?.maxHops ?? 8);
        const predIds = options?.predicates
            ? options.predicates
                .map((p) => this.store.getNodeIdByValue(p))
                .filter((x) => typeof x === 'number')
            : null;
        // 缓存查询结果，避免重复查询相同节点
        const forwardCache = new Map();
        const backwardCache = new Map();
        const neighborsForward = (nid) => {
            if (forwardCache.has(nid)) {
                return forwardCache.get(nid);
            }
            const out = [];
            const pushMatches = (criteria) => {
                const enc = this.store.query(criteria);
                out.push(...this.store.resolveRecords(enc));
            };
            if (predIds && predIds.length > 0) {
                for (const pid of predIds)
                    pushMatches({ subjectId: nid, predicateId: pid });
            }
            else {
                pushMatches({ subjectId: nid });
            }
            forwardCache.set(nid, out);
            return out;
        };
        const neighborsBackward = (nid) => {
            if (backwardCache.has(nid)) {
                return backwardCache.get(nid);
            }
            const out = [];
            const pushMatches = (criteria) => {
                const enc = this.store.query(criteria);
                out.push(...this.store.resolveRecords(enc));
            };
            if (predIds && predIds.length > 0) {
                for (const pid of predIds)
                    pushMatches({ predicateId: pid, objectId: nid });
            }
            else {
                pushMatches({ objectId: nid });
            }
            backwardCache.set(nid, out);
            return out;
        };
        // 使用 Map 提高查找效率，避免数组的线性搜索
        const prevFrom = new Map();
        const nextTo = new Map();
        const visitedFrom = new Set([startId]);
        const visitedTo = new Set([endId]);
        let frontierFrom = new Set([startId]);
        let frontierTo = new Set([endId]);
        let hops = 0;
        let meet = null;
        while (frontierFrom.size > 0 && frontierTo.size > 0 && hops < maxHops / 2 + 1) {
            hops += 1;
            // 选择较小的一侧扩展，提高效率
            if (frontierFrom.size <= frontierTo.size) {
                const nextFrontier = new Set();
                for (const u of frontierFrom) {
                    const neighbors = neighborsForward(u);
                    for (const e of neighbors) {
                        const v = e.objectId;
                        if (visitedFrom.has(v))
                            continue;
                        visitedFrom.add(v);
                        prevFrom.set(v, e);
                        // 检查是否与另一侧相遇
                        if (visitedTo.has(v)) {
                            meet = v;
                            break;
                        }
                        nextFrontier.add(v);
                    }
                    if (meet !== null)
                        break;
                }
                if (meet !== null)
                    break;
                frontierFrom = nextFrontier;
            }
            else {
                const nextFrontier = new Set();
                for (const u of frontierTo) {
                    const neighbors = neighborsBackward(u);
                    for (const e of neighbors) {
                        const v = e.subjectId; // 反向扩展得到上一节点
                        if (visitedTo.has(v))
                            continue;
                        visitedTo.add(v);
                        nextTo.set(v, e);
                        // 检查是否与另一侧相遇
                        if (visitedFrom.has(v)) {
                            meet = v;
                            break;
                        }
                        nextFrontier.add(v);
                    }
                    if (meet !== null)
                        break;
                }
                if (meet !== null)
                    break;
                frontierTo = nextFrontier;
            }
        }
        if (meet === null)
            return null;
        // 优化路径重建：使用单次遍历构建完整路径
        const path = [];
        // 回溯 start -> meet
        const leftPath = [];
        let cur = meet;
        while (cur !== startId && prevFrom.has(cur)) {
            const e = prevFrom.get(cur);
            leftPath.push(e);
            cur = e.subjectId;
        }
        // 正向遍历 start -> meet
        for (let i = leftPath.length - 1; i >= 0; i--) {
            path.push(leftPath[i]);
        }
        // 正向拼接 meet -> end
        cur = meet;
        while (cur !== endId && nextTo.has(cur)) {
            const e = nextTo.get(cur);
            path.push(e);
            cur = e.objectId;
        }
        return path;
    }
    // Dijkstra 加权最短路径（权重来自边属性，默认字段 'weight'，缺省视为1）
    shortestPathWeighted(from, to, options) {
        const startId = this.store.getNodeIdByValue(from);
        const endId = this.store.getNodeIdByValue(to);
        if (startId === undefined || endId === undefined)
            return null;
        const predicateId = options?.predicate
            ? this.store.getNodeIdByValue(options.predicate)
            : undefined;
        const weightKey = options?.weightProperty ?? 'weight';
        const dist = new Map();
        const prev = new Map();
        const visited = new Set();
        dist.set(startId, 0);
        // 使用最小堆优化优先队列性能
        const queue = new MinHeap((a, b) => a.d - b.d);
        queue.push({ node: startId, d: 0 });
        while (!queue.isEmpty()) {
            const { node } = queue.pop();
            if (visited.has(node))
                continue;
            visited.add(node);
            if (node === endId)
                break;
            const criteria = predicateId !== undefined ? { subjectId: node, predicateId } : { subjectId: node };
            const enc = this.store.query(criteria);
            const edges = this.store.resolveRecords(enc);
            for (const e of edges) {
                const rawWeight = e.edgeProperties ? e.edgeProperties[weightKey] : undefined;
                const w = Number(rawWeight ?? 1);
                const alt = (dist.get(node) ?? Infinity) + (Number.isFinite(w) ? w : 1);
                const v = e.objectId;
                if (alt < (dist.get(v) ?? Infinity)) {
                    dist.set(v, alt);
                    prev.set(v, e);
                    queue.push({ node: v, d: alt });
                }
            }
        }
        if (!dist.has(endId))
            return null;
        const path = [];
        let cur = endId;
        while (cur !== startId) {
            const edge = prev.get(cur);
            if (!edge)
                break;
            path.push(edge);
            cur = edge.subjectId;
        }
        path.reverse();
        return path;
    }
    // Cypher 极简子集：仅支持 MATCH (a)-[:REL]->(b) RETURN a,b
    cypher(query) {
        const m = /MATCH\s*\((\w+)\)\s*-\s*\[:(\w+)(?:\*(\d+)?\.\.(\d+)?)?\]\s*->\s*\((\w+)\)\s*RETURN\s+(.+)/i.exec(query);
        if (!m)
            throw new Error('仅支持最小子集：MATCH (a)-[:REL]->(b) RETURN ...');
        const aliasA = m[1];
        const rel = m[2];
        const minStr = m[3];
        const maxStr = m[4];
        const aliasB = m[5];
        const returnList = m[6].split(',').map((s) => s.trim());
        const hasVar = Boolean(minStr || maxStr);
        if (!hasVar) {
            const rows = this.find({ predicate: rel }).all();
            return rows.map((r) => {
                const env = {};
                const mapping = {
                    [aliasA]: r.subject,
                    [aliasB]: r.object,
                };
                for (const item of returnList)
                    env[item] = mapping[item] ?? null;
                return env;
            });
        }
        const min = minStr ? Number(minStr) : 1;
        const max = maxStr ? Number(maxStr) : min;
        const pid = this.store.getNodeIdByValue(rel);
        if (pid === undefined)
            return [];
        const startIds = new Set();
        const triples = this.find({ predicate: rel }).all();
        triples.forEach((t) => startIds.add(t.subjectId));
        const builder = new VariablePathBuilder(this.store, startIds, pid, {
            min,
            max,
            uniqueness: 'NODE',
            direction: 'forward',
        });
        const paths = builder.all();
        const out = [];
        for (const p of paths) {
            const env = {};
            const mapping = {
                [aliasA]: this.store.getNodeValueById(p.startId) ?? null,
                [aliasB]: this.store.getNodeValueById(p.endId) ?? null,
            };
            for (const item of returnList)
                env[item] = mapping[item] ?? null;
            out.push(env);
        }
        return out;
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