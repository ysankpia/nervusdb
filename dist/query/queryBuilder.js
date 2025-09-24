import { VariablePathBuilder } from './path/variable.js';
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
    pinnedEpoch;
    constructor(store, context, pinnedEpoch) {
        this.store = store;
        this.facts = context.facts;
        this.frontier = context.frontier;
        this.orientation = context.orientation;
        this.pinnedEpoch = pinnedEpoch;
    }
    // 变长路径构建器：返回 PathBuilder 用于获取路径结果
    variablePath(relation, options) {
        const predicateId = this.store.getNodeIdByValue(relation);
        if (predicateId === undefined || this.frontier.size === 0) {
            return new VariablePathBuilder(this.store, new Set(), 0, { min: 1, max: 1 });
        }
        return new VariablePathBuilder(this.store, new Set(this.frontier), predicateId, options);
    }
    // 为测试与直觉友好：提供数组化能力（只读视图）
    get length() {
        this.pin();
        try {
            return this.facts.length;
        }
        finally {
            this.unpin();
        }
    }
    slice(start, end) {
        this.pin();
        try {
            return this.facts.slice(start, end);
        }
        finally {
            this.unpin();
        }
    }
    // 迭代期间保持快照固定
    *[Symbol.iterator]() {
        this.pin();
        try {
            for (const fact of this.facts) {
                yield fact;
            }
        }
        finally {
            this.unpin();
        }
    }
    // 异步迭代器支持 - 流式查询
    async *[Symbol.asyncIterator]() {
        const pageSize = 2048;
        let offset = 0;
        this.pin();
        try {
            // 满足 require-await 规则，同时不改变逻辑
            await Promise.resolve();
            const total = this.facts.length;
            while (offset < total) {
                const end = Math.min(offset + pageSize, total);
                for (let i = offset; i < end; i++) {
                    yield this.facts[i];
                }
                offset = end;
            }
        }
        finally {
            this.unpin();
        }
    }
    toArray() {
        return this.all();
    }
    all() {
        this.pin();
        try {
            return [...this.facts];
        }
        finally {
            this.unpin();
        }
    }
    where(predicate) {
        this.pin();
        const nextFacts = this.facts.filter((f) => {
            try {
                return Boolean(predicate(f));
            }
            catch {
                return false;
            }
        });
        this.unpin();
        const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
        return new QueryBuilder(this.store, {
            facts: nextFacts,
            frontier: nextFrontier,
            orientation: this.orientation,
        }, this.pinnedEpoch);
    }
    // UNION：去重合并两个查询结果集
    union(other) {
        this.pin();
        other.pin();
        try {
            const map = new Map();
            for (const f of this.facts)
                map.set(encodeTripleKey(f), f);
            for (const f of other.facts)
                map.set(encodeTripleKey(f), f);
            const nextFacts = [...map.values()];
            const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
            return new QueryBuilder(this.store, { facts: nextFacts, frontier: nextFrontier, orientation: this.orientation }, this.pinnedEpoch);
        }
        finally {
            other.unpin();
            this.unpin();
        }
    }
    // UNION ALL：简单拼接不去重
    unionAll(other) {
        this.pin();
        other.pin();
        try {
            const nextFacts = [...this.facts, ...other.facts];
            const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
            return new QueryBuilder(this.store, { facts: nextFacts, frontier: nextFrontier, orientation: this.orientation }, this.pinnedEpoch);
        }
        finally {
            other.unpin();
            this.unpin();
        }
    }
    /**
     * 基于节点标签过滤当前结果集
     * @param labels 单个标签或标签数组
     * @param options 过滤选项：匹配模式与过滤对象
     * - mode: AND(默认) | OR
     * - on: 过滤作用于 subject | object | both(默认)
     */
    whereLabel(labels, options) {
        const labelArr = Array.isArray(labels) ? labels : [labels];
        const mode = options?.mode ?? 'AND';
        const on = options?.on ?? 'both';
        this.pin();
        try {
            const labelIndex = this.store.getLabelIndex();
            const nextFacts = this.facts.filter((fact) => {
                const testSubject = on === 'subject' || on === 'both'
                    ? mode === 'AND'
                        ? labelIndex.hasAllNodeLabels(fact.subjectId, labelArr)
                        : labelIndex.hasAnyNodeLabel(fact.subjectId, labelArr)
                    : false;
                const testObject = on === 'object' || on === 'both'
                    ? mode === 'AND'
                        ? labelIndex.hasAllNodeLabels(fact.objectId, labelArr)
                        : labelIndex.hasAnyNodeLabel(fact.objectId, labelArr)
                    : false;
                return testSubject || testObject;
            });
            const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
            return new QueryBuilder(this.store, { facts: nextFacts, frontier: nextFrontier, orientation: this.orientation }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    limit(n) {
        if (n < 0 || Number.isNaN(n)) {
            return this;
        }
        this.pin();
        const nextFacts = this.facts.slice(0, n);
        this.unpin();
        const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
        return new QueryBuilder(this.store, {
            facts: nextFacts,
            frontier: nextFrontier,
            orientation: this.orientation,
        }, this.pinnedEpoch);
    }
    // 流式查询方法 - take(n)
    take(n) {
        return this.limit(n);
    }
    // 流式查询方法 - skip(n)
    skip(n) {
        if (n <= 0 || Number.isNaN(n)) {
            return this;
        }
        this.pin();
        const nextFacts = this.facts.slice(n);
        this.unpin();
        const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
        return new QueryBuilder(this.store, {
            facts: nextFacts,
            frontier: nextFrontier,
            orientation: this.orientation,
        }, this.pinnedEpoch);
    }
    // 批量异步迭代器
    async *batch(size) {
        if (size <= 0) {
            throw new Error('批次大小必须大于 0');
        }
        this.pin();
        try {
            let offset = 0;
            while (offset < this.facts.length) {
                const batch = this.facts.slice(offset, offset + size);
                yield batch;
                offset += size;
                // 为流式处理添加小的延迟
                await new Promise((resolve) => setImmediate(resolve));
            }
        }
        finally {
            this.unpin();
        }
    }
    /**
     * 属性索引下推查询 - 通用接口
     * @param propertyName 属性名
     * @param operator 操作符
     * @param value 值
     * @param target 查询目标（节点或边）
     */
    whereProperty(propertyName, operator, value, target = 'node') {
        this.pin();
        try {
            const propertyIndex = this.store.getPropertyIndex();
            let matchingIds;
            if (target === 'node') {
                if (operator === '=') {
                    matchingIds = propertyIndex.queryNodesByProperty(propertyName, value);
                }
                else {
                    // 范围查询
                    const { min, max, includeMin, includeMax } = this.buildRangeFromOperator(operator, value);
                    matchingIds = propertyIndex.queryNodesByRange(propertyName, min, max, includeMin, includeMax);
                }
                // 节点属性查询：同时检查主体和客体，不依赖可能有误导性的orientation
                const nextFacts = this.facts.filter((fact) => {
                    return matchingIds.has(fact.subjectId) || matchingIds.has(fact.objectId);
                });
                const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
                return new QueryBuilder(this.store, {
                    facts: nextFacts,
                    frontier: nextFrontier,
                    orientation: this.orientation,
                }, this.pinnedEpoch);
            }
            else {
                // target === 'edge'
                if (operator === '=') {
                    matchingIds = propertyIndex.queryEdgesByProperty(propertyName, value);
                }
                else {
                    // 边属性暂不支持范围查询
                    throw new Error('边属性暂不支持范围查询操作符');
                }
                // 过滤当前事实
                const nextFacts = this.facts.filter((fact) => {
                    const edgeKey = encodeTripleKey(fact);
                    return matchingIds.has(edgeKey);
                });
                const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
                return new QueryBuilder(this.store, {
                    facts: nextFacts,
                    frontier: nextFrontier,
                    orientation: this.orientation,
                }, this.pinnedEpoch);
            }
        }
        finally {
            this.unpin();
        }
    }
    /**
     * 根据操作符构建范围查询参数
     */
    buildRangeFromOperator(operator, value) {
        switch (operator) {
            case '>':
                return { min: value, max: undefined, includeMin: false, includeMax: true };
            case '>=':
                return { min: value, max: undefined, includeMin: true, includeMax: true };
            case '<':
                return { min: undefined, max: value, includeMin: true, includeMax: false };
            case '<=':
                return { min: undefined, max: value, includeMin: true, includeMax: true };
            default:
                // 理论上不会触达（已穷举四种操作符）
                throw new Error('不支持的操作符');
        }
    }
    /**
     * 根据节点属性过滤当前前沿
     * @param filter 属性过滤条件
     */
    whereNodeProperty(filter) {
        this.pin();
        try {
            const propertyIndex = this.store.getPropertyIndex();
            let matchingNodeIds;
            if (filter.value !== undefined) {
                // 等值查询
                matchingNodeIds = propertyIndex.queryNodesByProperty(filter.propertyName, filter.value);
            }
            else if (filter.range) {
                // 范围查询
                matchingNodeIds = propertyIndex.queryNodesByRange(filter.propertyName, filter.range.min, filter.range.max, filter.range.includeMin, filter.range.includeMax);
            }
            else {
                // 如果没有指定值或范围，返回所有具有该属性的节点
                const allPropertyNames = propertyIndex.getNodePropertyNames();
                if (!allPropertyNames.includes(filter.propertyName)) {
                    return new QueryBuilder(this.store, EMPTY_CONTEXT, this.pinnedEpoch);
                }
                // 获取所有具有该属性的节点（通过查询所有可能的值）
                matchingNodeIds = new Set();
                // 注意：这是一个简化实现，在实际应用中可能需要更高效的方式
            }
            // 节点属性查询：同时检查主体和客体，不依赖可能有误导性的orientation
            const nextFacts = this.facts.filter((fact) => {
                return matchingNodeIds.has(fact.subjectId) || matchingNodeIds.has(fact.objectId);
            });
            const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
            return new QueryBuilder(this.store, {
                facts: nextFacts,
                frontier: nextFrontier,
                orientation: this.orientation,
            }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    /**
     * 根据边属性过滤当前事实
     * @param filter 属性过滤条件
     */
    whereEdgeProperty(filter) {
        this.pin();
        try {
            const propertyIndex = this.store.getPropertyIndex();
            let matchingEdgeKeys;
            if (filter.value !== undefined) {
                // 等值查询
                matchingEdgeKeys = propertyIndex.queryEdgesByProperty(filter.propertyName, filter.value);
            }
            else {
                // 如果没有指定值，返回所有具有该属性的边
                const allPropertyNames = propertyIndex.getEdgePropertyNames();
                if (!allPropertyNames.includes(filter.propertyName)) {
                    return new QueryBuilder(this.store, EMPTY_CONTEXT, this.pinnedEpoch);
                }
                // 获取所有具有该属性的边
                matchingEdgeKeys = new Set();
                // 注意：这是一个简化实现
            }
            // 过滤当前事实
            const nextFacts = this.facts.filter((fact) => {
                const edgeKey = encodeTripleKey(fact);
                return matchingEdgeKeys.has(edgeKey);
            });
            const nextFrontier = rebuildFrontier(nextFacts, this.orientation);
            return new QueryBuilder(this.store, {
                facts: nextFacts,
                frontier: nextFrontier,
                orientation: this.orientation,
            }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    /**
     * 基于属性条件进行联想查询
     * @param predicate 关系谓词
     * @param nodePropertyFilter 可选的目标节点属性过滤条件
     */
    followWithNodeProperty(predicate, nodePropertyFilter) {
        return this.traverseWithProperty(predicate, 'forward', nodePropertyFilter);
    }
    /**
     * 基于属性条件进行反向联想查询
     * @param predicate 关系谓词
     * @param nodePropertyFilter 可选的目标节点属性过滤条件
     */
    followReverseWithNodeProperty(predicate, nodePropertyFilter) {
        return this.traverseWithProperty(predicate, 'reverse', nodePropertyFilter);
    }
    /**
     * 带属性过滤的联想查询实现
     */
    traverseWithProperty(predicate, direction, nodePropertyFilter) {
        if (this.frontier.size === 0) {
            return new QueryBuilder(this.store, EMPTY_CONTEXT);
        }
        this.pin();
        try {
            const predicateId = this.store.getNodeIdByValue(predicate);
            if (predicateId === undefined) {
                return new QueryBuilder(this.store, EMPTY_CONTEXT);
            }
            // 如果有节点属性过滤条件，先获取匹配的节点ID
            let targetNodeIds;
            if (nodePropertyFilter) {
                const propertyIndex = this.store.getPropertyIndex();
                if (nodePropertyFilter.value !== undefined) {
                    targetNodeIds = propertyIndex.queryNodesByProperty(nodePropertyFilter.propertyName, nodePropertyFilter.value);
                }
                else if (nodePropertyFilter.range) {
                    targetNodeIds = propertyIndex.queryNodesByRange(nodePropertyFilter.propertyName, nodePropertyFilter.range.min, nodePropertyFilter.range.max, nodePropertyFilter.range.includeMin, nodePropertyFilter.range.includeMax);
                }
                // 如果没有匹配的节点，直接返回空结果
                if (targetNodeIds && targetNodeIds.size === 0) {
                    return new QueryBuilder(this.store, EMPTY_CONTEXT);
                }
            }
            const triples = new Map();
            for (const nodeId of this.frontier.values()) {
                const criteria = direction === 'forward'
                    ? { subjectId: nodeId, predicateId }
                    : { predicateId, objectId: nodeId };
                const matches = this.store.query(criteria);
                const records = this.store.resolveRecords(matches);
                records.forEach((record) => {
                    // 如果有目标节点过滤条件，检查目标节点是否匹配
                    if (targetNodeIds) {
                        const targetNodeId = direction === 'forward' ? record.objectId : record.subjectId;
                        if (!targetNodeIds.has(targetNodeId)) {
                            return; // 跳过不匹配的记录
                        }
                    }
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
            }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    anchor(orientation) {
        this.pin();
        const nextFrontier = buildInitialFrontier(this.facts, orientation);
        this.unpin();
        return new QueryBuilder(this.store, {
            facts: [...this.facts],
            frontier: nextFrontier,
            orientation,
        }, this.pinnedEpoch);
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
        this.pin();
        try {
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
            }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    /**
     * 变长路径查询：支持 [min..max] 跳数的同谓词遍历
     * 默认正向遍历，返回满足跳数范围的“最后一跳”三元组集合
     */
    followPath(predicate, range, options) {
        if (this.frontier.size === 0)
            return new QueryBuilder(this.store, EMPTY_CONTEXT);
        const min = Math.max(1, range.min ?? 1);
        const max = Math.max(min, range.max);
        const direction = options?.direction ?? 'forward';
        this.pin();
        try {
            const predicateId = this.store.getNodeIdByValue(predicate);
            if (predicateId === undefined)
                return new QueryBuilder(this.store, EMPTY_CONTEXT);
            // BFS 按层扩展
            let currentFrontier = new Set(this.frontier);
            const visited = new Set();
            const resultTriples = new Map();
            let depth = 0;
            while (depth < max && currentFrontier.size > 0) {
                depth += 1;
                const nextFrontier = new Set();
                for (const nodeId of currentFrontier) {
                    // 防止爆炸性重复扩展：节点级去重
                    if (visited.has(nodeId))
                        continue;
                    visited.add(nodeId);
                    const criteria = direction === 'forward'
                        ? { subjectId: nodeId, predicateId }
                        : { predicateId, objectId: nodeId };
                    const matches = this.store.query(criteria);
                    const records = this.store.resolveRecords(matches);
                    for (const rec of records) {
                        // 处于允许的范围时，收集“最后一跳”的边
                        if (depth >= min) {
                            resultTriples.set(encodeTripleKey(rec), rec);
                        }
                        // 推进下一层前沿
                        const nextNode = direction === 'forward' ? rec.objectId : rec.subjectId;
                        nextFrontier.add(nextNode);
                    }
                }
                currentFrontier = nextFrontier;
            }
            const nextFacts = [...resultTriples.values()];
            const nextFrontierSet = new Set();
            for (const rec of nextFacts) {
                nextFrontierSet.add(direction === 'forward' ? rec.objectId : rec.subjectId);
            }
            return new QueryBuilder(this.store, {
                facts: nextFacts,
                frontier: nextFrontierSet,
                orientation: direction === 'forward' ? 'object' : 'subject',
            }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    static fromFindResult(store, context, pinnedEpoch) {
        return new QueryBuilder(store, context, pinnedEpoch);
    }
    static empty(store) {
        return new QueryBuilder(store, EMPTY_CONTEXT);
    }
    pin() {
        if (this.pinnedEpoch !== undefined) {
            try {
                // 只做内存级别的epoch固定，避免与withSnapshot的reader注册冲突
                this.store.pinnedEpochStack?.push(this.pinnedEpoch);
            }
            catch {
                /* ignore */
            }
        }
    }
    unpin() {
        if (this.pinnedEpoch !== undefined) {
            try {
                // 只做内存级别的epoch释放，避免与withSnapshot的reader注册冲突
                this.store.pinnedEpochStack?.pop();
            }
            catch {
                /* ignore */
            }
        }
    }
}
/**
 * 流式查询构建器 - 真正的内存高效流式查询
 */
export class StreamingQueryBuilder {
    store;
    factsStream;
    frontier;
    orientation;
    pinnedEpoch;
    constructor(store, context, pinnedEpoch) {
        this.store = store;
        this.factsStream = context.factsStream;
        this.frontier = context.frontier;
        this.orientation = context.orientation;
        this.pinnedEpoch = pinnedEpoch;
    }
    /**
     * 真正的流式异步迭代器 - 逐条处理，不预加载所有数据
     */
    async *[Symbol.asyncIterator]() {
        this.pin();
        try {
            // 直接流式迭代，不预加载到内存
            for await (const fact of this.factsStream) {
                yield fact;
            }
        }
        finally {
            this.unpin();
        }
    }
    /**
     * 转换为普通 QueryBuilder（向后兼容）
     */
    async toQueryBuilder() {
        const facts = [];
        this.pin();
        try {
            for await (const fact of this.factsStream) {
                facts.push(fact);
            }
            return new QueryBuilder(this.store, { facts, frontier: this.frontier, orientation: this.orientation }, this.pinnedEpoch);
        }
        finally {
            this.unpin();
        }
    }
    pin() {
        if (this.pinnedEpoch !== undefined) {
            this.store.pinnedEpochStack?.push(this.pinnedEpoch);
        }
    }
    unpin() {
        if (this.pinnedEpoch !== undefined) {
            try {
                this.store.pinnedEpochStack?.pop();
            }
            catch {
                /* ignore */
            }
        }
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
    const includeProps = !(query.subjectId === undefined &&
        query.predicateId === undefined &&
        query.objectId === undefined);
    const facts = store.resolveRecords(matches, { includeProperties: includeProps });
    const frontier = buildInitialFrontier(facts, anchor);
    return {
        facts,
        frontier,
        orientation: anchor,
    };
}
/**
 * 构建流式查询上下文 - 真正的内存高效查询
 */
export async function buildStreamingFindContext(store, criteria, anchor) {
    // 保持异步 API 形态；满足 require-await
    await Promise.resolve();
    const query = convertCriteriaToIds(store, criteria);
    if (query === null) {
        return {
            factsStream: (async function* () { })(),
            frontier: new Set(),
            orientation: anchor,
        };
    }
    // 使用流式查询替代预加载，需要转换为 FactRecord
    const factsStream = store.queryStreaming(query);
    const frontier = new Set(); // 流式查询需要动态构建前沿
    // 将 EncodedTriple 转换为 FactRecord 的流式包装器
    async function* encodeTripleToFactRecord(stream) {
        for await (const triple of stream) {
            // 这里需要转换逻辑，但现在先简化处理
            // 注意：这是一个临时解决方案，更好的方法是在 PersistentStore 层提供 FactRecord 的流式查询
            const fact = store.resolveRecords([triple])[0];
            if (fact)
                yield fact;
        }
    }
    return {
        factsStream: encodeTripleToFactRecord(factsStream),
        frontier,
        orientation: anchor,
    };
}
/**
 * 基于属性条件构建查询上下文
 * @param store 数据存储实例
 * @param propertyFilter 属性过滤条件
 * @param anchor 前沿方向
 * @param target 查询目标（节点或边）
 */
export function buildFindContextFromProperty(store, propertyFilter, anchor, target = 'node') {
    const propertyIndex = store.getPropertyIndex();
    if (target === 'node') {
        let matchingNodeIds;
        if (propertyFilter.value !== undefined) {
            // 等值查询
            matchingNodeIds = propertyIndex.queryNodesByProperty(propertyFilter.propertyName, propertyFilter.value);
        }
        else if (propertyFilter.range) {
            // 范围查询
            matchingNodeIds = propertyIndex.queryNodesByRange(propertyFilter.propertyName, propertyFilter.range.min, propertyFilter.range.max, propertyFilter.range.includeMin, propertyFilter.range.includeMax);
        }
        else {
            // 返回所有具有该属性的节点
            const allPropertyNames = propertyIndex.getNodePropertyNames();
            if (!allPropertyNames.includes(propertyFilter.propertyName)) {
                return EMPTY_CONTEXT;
            }
            matchingNodeIds = new Set();
            // 注意：这需要更完整的实现来获取所有具有该属性的节点
        }
        if (matchingNodeIds.size === 0) {
            return EMPTY_CONTEXT;
        }
        // 查找包含这些节点的所有三元组
        const allFacts = [];
        for (const nodeId of matchingNodeIds) {
            // 作为主语的三元组
            const subjectTriples = store.query({ subjectId: nodeId });
            allFacts.push(...store.resolveRecords(subjectTriples));
            // 作为宾语的三元组
            const objectTriples = store.query({ objectId: nodeId });
            allFacts.push(...store.resolveRecords(objectTriples));
        }
        // 去重
        const uniqueFacts = new Map();
        allFacts.forEach((fact) => {
            uniqueFacts.set(encodeTripleKey(fact), fact);
        });
        const facts = [...uniqueFacts.values()];
        const frontier = buildInitialFrontier(facts, anchor);
        return {
            facts,
            frontier,
            orientation: anchor,
        };
    }
    else {
        // target === 'edge'
        let matchingEdgeKeys;
        if (propertyFilter.value !== undefined) {
            matchingEdgeKeys = propertyIndex.queryEdgesByProperty(propertyFilter.propertyName, propertyFilter.value);
        }
        else {
            const allPropertyNames = propertyIndex.getEdgePropertyNames();
            if (!allPropertyNames.includes(propertyFilter.propertyName)) {
                return EMPTY_CONTEXT;
            }
            matchingEdgeKeys = new Set();
            // 注意：这需要更完整的实现
        }
        if (matchingEdgeKeys.size === 0) {
            return EMPTY_CONTEXT;
        }
        // 根据边键获取对应的三元组
        const facts = [];
        for (const edgeKey of matchingEdgeKeys) {
            const [subjectId, predicateId, objectId] = edgeKey.split(':').map(Number);
            const matches = store.query({ subjectId, predicateId, objectId });
            facts.push(...store.resolveRecords(matches));
        }
        const frontier = buildInitialFrontier(facts, anchor);
        return {
            facts,
            frontier,
            orientation: anchor,
        };
    }
}
/**
 * 基于标签条件构建查询上下文
 * @param store 数据存储实例
 * @param labels 单个或多个标签
 * @param options 模式：AND/OR
 * @param anchor 前沿方向
 */
export function buildFindContextFromLabel(store, labels, options, anchor) {
    const labelIndex = store.getLabelIndex();
    const arr = Array.isArray(labels) ? labels : [labels];
    const mode = options?.mode ?? 'AND';
    let nodeIds;
    if (arr.length === 0)
        return EMPTY_CONTEXT;
    if (arr.length === 1 && mode === 'AND') {
        nodeIds = labelIndex.findNodesByLabel(arr[0]);
    }
    else {
        nodeIds = labelIndex.findNodesByLabels(arr, { mode });
    }
    if (nodeIds.size === 0)
        return EMPTY_CONTEXT;
    const triples = new Map();
    for (const nodeId of nodeIds) {
        const sMatches = store.query({ subjectId: nodeId });
        for (const rec of store.resolveRecords(sMatches)) {
            triples.set(encodeTripleKey(rec), rec);
        }
        const oMatches = store.query({ objectId: nodeId });
        for (const rec of store.resolveRecords(oMatches)) {
            triples.set(encodeTripleKey(rec), rec);
        }
    }
    const facts = [...triples.values()];
    const frontier = buildInitialFrontier(facts, anchor);
    return { facts, frontier, orientation: anchor };
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