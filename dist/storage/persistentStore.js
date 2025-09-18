import { promises as fsp } from 'node:fs';
import { join } from 'node:path';
import { initializeIfMissing, readStorageFile, writeStorageFile } from './fileHeader';
import { StringDictionary } from './dictionary';
import { PropertyStore } from './propertyStore';
import { TripleIndexes, getBestIndexKey } from './tripleIndexes';
import { TripleStore } from './tripleStore';
import { LsmLiteStaging } from './staging';
import { PagedIndexReader, PagedIndexWriter, pageFileName, readPagedManifest, writePagedManifest, DEFAULT_PAGE_SIZE, } from './pagedIndex';
import { WalReplayer, WalWriter } from './wal';
import { readHotness, writeHotness } from './hotness';
import { addReader, removeReader, cleanupProcessReaders } from './readerRegistry';
import { acquireLock } from '../utils/lock';
import { triggerCrash } from '../utils/fault';
export class PersistentStore {
    path;
    dictionary;
    triples;
    properties;
    indexes;
    indexDirectory;
    constructor(path, dictionary, triples, properties, indexes, indexDirectory) {
        this.path = path;
        this.dictionary = dictionary;
        this.triples = triples;
        this.properties = properties;
        this.indexes = indexes;
        this.indexDirectory = indexDirectory;
    }
    dirty = false;
    wal;
    tombstones = new Set();
    hotness = null;
    lock;
    batchDepth = 0;
    batchMetaStack = [];
    // 事务暂存栈：支持嵌套批次，commit 向外层合并，最外层 commit 落入主存；abort 丢弃
    txStack = [];
    currentEpoch = 0;
    lastManifestCheck = 0;
    pinnedEpochStack = [];
    readerRegistered = false;
    snapshotRefCount = 0;
    activeReaderOperation = null;
    lsm;
    static async open(path, options = {}) {
        await initializeIfMissing(path);
        // 当存在写锁且尝试以无锁模式打开时，若 WAL 非空（存在未落盘的写入），拒绝无锁访问
        // 用于防止已加锁写者运行期间，第二个“伪读者”的无锁写入引发并发风险
        try {
            if (options.enableLock === false) {
                const lockPath = `${path}.lock`;
                const walPath = `${path}.wal`;
                // 检查锁文件是否存在
                const [lstat, wstat] = await Promise.allSettled([fsp.stat(lockPath), fsp.stat(walPath)]);
                const locked = lstat.status === 'fulfilled';
                const walSize = wstat.status === 'fulfilled' ? (wstat.value.size ?? 0) : 0;
                // WAL header 固定 12 字节；大于 12 说明存在未 reset 的写入
                if (locked && walSize > 12) {
                    throw new Error('数据库当前由写者持有锁且存在未落盘的 WAL 写入，禁止无锁打开。请等待写者 flush/释放后再以读者模式访问。');
                }
            }
        }
        catch {
            // 防御性：出现异常时不影响正常打开流程
        }
        const sections = await readStorageFile(path);
        const dictionary = StringDictionary.deserialize(sections.dictionary);
        const triples = TripleStore.deserialize(sections.triples);
        const propertyStore = PropertyStore.deserialize(sections.properties);
        const indexes = TripleIndexes.deserialize(sections.indexes);
        // 初次打开且无 manifest 时，将以全量方式重建分页索引，无需在内存中保有全部索引
        const indexDirectory = options.indexDirectory ?? `${path}.pages`;
        // 清理当前进程可能残留的旧reader文件（防止上次异常退出的残留）
        try {
            await cleanupProcessReaders(indexDirectory, process.pid);
        }
        catch {
            // 忽略清理错误，不影响数据库打开
        }
        const store = new PersistentStore(path, dictionary, triples, propertyStore, indexes, indexDirectory);
        if (options.enableLock) {
            store.lock = await acquireLock(path);
        }
        if (options.stagingMode === 'lsm-lite') {
            store.lsm = new LsmLiteStaging();
        }
        // WAL 重放（将未持久化的增量恢复到内存与 staging）
        store.wal = await WalWriter.open(path);
        // 持久 txId 去重：读取注册表（可选）
        const { readTxIdRegistry, writeTxIdRegistry, toSet, mergeTxIds } = await import('./txidRegistry');
        const persistentTx = options.enablePersistentTxDedupe === true;
        const maxTx = options.maxRememberTxIds ?? 1000;
        const reg = persistentTx ? await readTxIdRegistry(indexDirectory) : { version: 1, txIds: [] };
        const knownTx = persistentTx ? toSet(reg) : undefined;
        const replay = await new WalReplayer(path).replay(knownTx);
        for (const f of replay.addFacts)
            store.addFactDirect(f);
        for (const f of replay.deleteFacts)
            store.deleteFactDirect(f);
        for (const n of replay.nodeProps)
            store.setNodePropertiesDirect(n.nodeId, n.value);
        for (const e of replay.edgeProps)
            store.setEdgePropertiesDirect(e.ids, e.value);
        // 截断 WAL 尾部不完整记录，确保下次打开幂等
        if (replay.safeOffset > 0) {
            await store.wal.truncateTo(replay.safeOffset);
        }
        // 将本次重放新增的 txId 合并入注册表
        if (persistentTx && replay.committedTx.length > 0) {
            const merged = mergeTxIds(reg, replay.committedTx.map((x) => ({ id: x.id, sessionId: x.sessionId })), maxTx);
            await writeTxIdRegistry(indexDirectory, merged);
        }
        const manifest = await readPagedManifest(indexDirectory);
        const shouldRebuild = options.rebuildIndexes === true ||
            !manifest ||
            manifest.pageSize !== (options.pageSize ?? DEFAULT_PAGE_SIZE);
        if (shouldRebuild) {
            await store.buildPagedIndexes(options.pageSize, options.compression);
        }
        else {
            store.hydratePagedReaders(manifest);
            store.currentEpoch = manifest.epoch ?? 0;
        }
        // 加载热度计数
        try {
            store.hotness = await readHotness(indexDirectory);
        }
        catch {
            store.hotness = {
                version: 1,
                updatedAt: Date.now(),
                counts: { SPO: {}, SOP: {}, POS: {}, PSO: {}, OSP: {}, OPS: {} },
            };
        }
        if (options.registerReader !== false) {
            await addReader(indexDirectory, {
                pid: process.pid,
                epoch: store.currentEpoch,
                ts: Date.now(),
            });
            store.readerRegistered = true;
        }
        return store;
    }
    pagedReaders = new Map();
    hydratePagedReaders(manifest) {
        for (const lookup of manifest.lookups) {
            this.pagedReaders.set(lookup.order, new PagedIndexReader({ directory: this.indexDirectory, compression: manifest.compression }, lookup));
        }
        if (manifest.tombstones && manifest.tombstones.length > 0) {
            manifest.tombstones.forEach(([subjectId, predicateId, objectId]) => {
                this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
            });
        }
    }
    async buildPagedIndexes(pageSize = DEFAULT_PAGE_SIZE, compression = { codec: 'none' }) {
        await fsp.mkdir(this.indexDirectory, { recursive: true });
        const orders = ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
        const lookups = [];
        for (const order of orders) {
            const filePath = join(this.indexDirectory, pageFileName(order));
            try {
                await fsp.unlink(filePath);
            }
            catch {
                /* noop */
            }
            const writer = new PagedIndexWriter(filePath, {
                directory: this.indexDirectory,
                pageSize,
                compression,
            });
            // 初次/重建：写入“全量”三元组（当前从 TripleStore 一次性构建）
            const triples = this.triples.list();
            const getPrimary = primarySelector(order);
            for (const t of triples) {
                writer.push(t, getPrimary(t));
            }
            const pages = await writer.finalize();
            this.pagedReaders.set(order, new PagedIndexReader({ directory: this.indexDirectory, compression }, { order, pages }));
            lookups.push({ order, pages });
        }
        const manifest = {
            version: 1,
            pageSize,
            createdAt: Date.now(),
            compression,
            lookups,
        };
        await writePagedManifest(this.indexDirectory, manifest);
    }
    async appendPagedIndexesFromStaging(pageSize = DEFAULT_PAGE_SIZE) {
        await fsp.mkdir(this.indexDirectory, { recursive: true });
        const manifest = (await readPagedManifest(this.indexDirectory)) ?? {
            version: 1,
            pageSize,
            createdAt: Date.now(),
            compression: { codec: 'none' },
            lookups: [],
        };
        // 若未显式传入，则沿用 manifest.pageSize，避免与初建不一致
        const effectivePageSize = pageSize === DEFAULT_PAGE_SIZE && manifest.pageSize ? manifest.pageSize : pageSize;
        const lookupMap = new Map(manifest.lookups.map((l) => [l.order, { order: l.order, pages: l.pages }]));
        // 实验性：读取 LSM 段，尝试在本轮一并合并到分页索引
        let lsmTriples = [];
        let lsmSegmentsToRemove = [];
        try {
            const manPath = join(this.indexDirectory, 'lsm-manifest.json');
            const buf = await fsp.readFile(manPath);
            const lsmMan = JSON.parse(buf.toString('utf8'));
            for (const seg of lsmMan.segments ?? []) {
                const filePath = join(this.indexDirectory, seg.file);
                try {
                    const data = await fsp.readFile(filePath);
                    const cnt = Math.floor(data.length / 12);
                    for (let i = 0; i < cnt; i += 1) {
                        const off = i * 12;
                        lsmTriples.push({
                            subjectId: data.readUInt32LE(off),
                            predicateId: data.readUInt32LE(off + 4),
                            objectId: data.readUInt32LE(off + 8),
                        });
                    }
                    lsmSegmentsToRemove.push(filePath);
                }
                catch {
                    // 单个段读取失败忽略
                }
            }
        }
        catch {
            // 无 LSM 段或清单缺失
        }
        const orders = ['SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS'];
        for (const order of orders) {
            const staged = this.indexes.get(order);
            const segs = lsmTriples;
            if (staged.length === 0 && segs.length === 0)
                continue;
            const filePath = join(this.indexDirectory, pageFileName(order));
            const writer = new PagedIndexWriter(filePath, {
                directory: this.indexDirectory,
                pageSize: effectivePageSize,
                compression: manifest.compression,
            });
            const getPrimary = primarySelector(order);
            for (const t of staged)
                writer.push(t, getPrimary(t));
            for (const t of segs)
                writer.push(t, getPrimary(t));
            const newPages = await writer.finalize();
            const existed = lookupMap.get(order) ?? { order, pages: [] };
            existed.pages.push(...newPages);
            lookupMap.set(order, existed);
        }
        const lookups = [...lookupMap.values()];
        const newManifest = {
            version: 1,
            pageSize: effectivePageSize,
            createdAt: Date.now(),
            compression: manifest.compression,
            lookups,
            epoch: (manifest.epoch ?? 0) + 1,
        };
        await writePagedManifest(this.indexDirectory, newManifest);
        this.hydratePagedReaders(newManifest);
        this.currentEpoch = newManifest.epoch ?? this.currentEpoch;
        // 清空 staging
        this.indexes.seed([]);
        // 实验性：清理已合并的 LSM 段并重置清单
        if (lsmSegmentsToRemove.length > 0) {
            try {
                for (const f of lsmSegmentsToRemove) {
                    try {
                        await fsp.unlink(f);
                    }
                    catch { }
                }
                const manPath = join(this.indexDirectory, 'lsm-manifest.json');
                await fsp.writeFile(manPath, JSON.stringify({ version: 1, segments: [] }, null, 2), 'utf8');
            }
            catch {
                // 忽略清理失败
            }
        }
    }
    addFact(fact) {
        // 仅写 WAL 记录；若处于批次中，则暂存到 txStack，最外层 commit 时再落入主存
        const inBatch = this.batchDepth > 0;
        void this.wal.appendAddTriple(fact);
        const subjectId = this.dictionary.getOrCreateId(fact.subject);
        const predicateId = this.dictionary.getOrCreateId(fact.predicate);
        const objectId = this.dictionary.getOrCreateId(fact.object);
        const triple = {
            subjectId,
            predicateId,
            objectId,
        };
        if (inBatch) {
            // 暂存，不立即变更主存
            const tx = this.peekTx();
            if (tx)
                tx.adds.push(triple);
        }
        else {
            if (!this.triples.has(triple)) {
                this.triples.add(triple);
                this.stageAdd(triple);
                this.dirty = true;
            }
        }
        return {
            ...fact,
            subjectId,
            predicateId,
            objectId,
        };
    }
    addFactDirect(fact) {
        const subjectId = this.dictionary.getOrCreateId(fact.subject);
        const predicateId = this.dictionary.getOrCreateId(fact.predicate);
        const objectId = this.dictionary.getOrCreateId(fact.object);
        const triple = {
            subjectId,
            predicateId,
            objectId,
        };
        if (!this.triples.has(triple)) {
            this.triples.add(triple);
            this.stageAdd(triple);
            this.dirty = true;
        }
        else {
            // 已存在于主文件：为了查询可见性，仍将其加入暂存索引并标记脏，直到下一次 flush 合并分页
            this.stageAdd(triple);
            this.dirty = true;
        }
        return {
            ...fact,
            subjectId,
            predicateId,
            objectId,
        };
    }
    listFacts() {
        return this.resolveRecords(this.triples.list());
    }
    getDictionarySize() {
        return this.dictionary.size;
    }
    getNodeIdByValue(value) {
        return this.dictionary.getId(value);
    }
    getNodeValueById(id) {
        return this.dictionary.getValue(id);
    }
    deleteFact(fact) {
        const inBatch = this.batchDepth > 0;
        void this.wal.appendDeleteTriple(fact);
        if (inBatch) {
            const subjectId = this.dictionary.getOrCreateId(fact.subject);
            const predicateId = this.dictionary.getOrCreateId(fact.predicate);
            const objectId = this.dictionary.getOrCreateId(fact.object);
            const triple = { subjectId, predicateId, objectId };
            const tx = this.peekTx();
            if (tx)
                tx.dels.push(triple);
        }
        else {
            this.deleteFactDirect(fact);
        }
    }
    deleteFactDirect(fact) {
        const subjectId = this.dictionary.getOrCreateId(fact.subject);
        const predicateId = this.dictionary.getOrCreateId(fact.predicate);
        const objectId = this.dictionary.getOrCreateId(fact.object);
        this.tombstones.add(encodeTripleKey({ subjectId, predicateId, objectId }));
        this.dirty = true;
    }
    setNodeProperties(nodeId, properties) {
        const inBatch = this.batchDepth > 0;
        void this.wal.appendSetNodeProps(nodeId, properties);
        if (inBatch) {
            const tx = this.peekTx();
            if (tx)
                tx.nodeProps.set(nodeId, properties);
        }
        else {
            this.properties.setNodeProperties(nodeId, properties);
            this.dirty = true;
        }
    }
    setEdgeProperties(key, properties) {
        const inBatch = this.batchDepth > 0;
        void this.wal.appendSetEdgeProps(key, properties);
        if (inBatch) {
            const tx = this.peekTx();
            if (tx)
                tx.edgeProps.set(encodeTripleKey(key), properties);
        }
        else {
            this.properties.setEdgeProperties(key, properties);
            this.dirty = true;
        }
    }
    // 事务批次（可选）：外部可将多条写入合并为一个 WAL 批次
    beginBatch(options) {
        // 记录每一层的 BEGIN（含可选 tx 元信息），便于 WAL 重放时支持嵌套语义
        void this.wal.appendBegin(options);
        this.batchDepth += 1;
        this.batchMetaStack.push({ txId: options?.txId, sessionId: options?.sessionId });
        this.txStack.push({
            adds: [],
            dels: [],
            nodeProps: new Map(),
            edgeProps: new Map(),
        });
    }
    commitBatch(options) {
        if (this.batchDepth > 0)
            this.batchDepth -= 1;
        const stage = this.txStack.pop();
        // 将提交记录写入 WAL（内层也记录，以支持重放栈语义）
        if (options?.durable)
            void this.wal.appendCommitDurable();
        else
            void this.wal.appendCommit();
        if (this.batchDepth === 0) {
            // 最外层提交：将暂存应用到主存
            if (stage)
                this.applyStage(stage);
        }
        else {
            // 嵌套提交：合并到上层
            const parent = this.peekTx();
            if (stage && parent) {
                parent.adds.push(...stage.adds);
                parent.dels.push(...stage.dels);
                stage.nodeProps.forEach((v, k) => parent.nodeProps.set(k, v));
                stage.edgeProps.forEach((v, k) => parent.edgeProps.set(k, v));
            }
        }
        // 持久 txId 去重：记录本次 txId
        const meta = this.batchMetaStack.pop();
        if (meta?.txId) {
            void (async () => {
                try {
                    const { readTxIdRegistry, writeTxIdRegistry, mergeTxIds } = await import('./txidRegistry');
                    const reg = await readTxIdRegistry(this.indexDirectory);
                    const merged = mergeTxIds(reg, [{ id: meta.txId, sessionId: meta.sessionId }], undefined);
                    await writeTxIdRegistry(this.indexDirectory, merged);
                }
                catch {
                    /* ignore registry error */
                }
            })();
        }
    }
    abortBatch() {
        // 放弃当前顶层批次（仅一层），支持嵌套部分回滚
        if (this.batchDepth <= 0)
            return;
        this.batchDepth -= 1;
        void this.wal.appendAbort();
        // 丢弃当前层暂存与元信息
        this.batchMetaStack.pop();
        this.txStack.pop();
    }
    setNodePropertiesDirect(nodeId, properties) {
        this.properties.setNodeProperties(nodeId, properties);
        this.dirty = true;
    }
    setEdgePropertiesDirect(key, properties) {
        this.properties.setEdgeProperties(key, properties);
        this.dirty = true;
    }
    getNodeProperties(nodeId) {
        // 若处于事务中，优先返回顶层事务暂存视图
        for (let i = this.txStack.length - 1; i >= 0; i -= 1) {
            const v = this.txStack[i].nodeProps.get(nodeId);
            if (v !== undefined)
                return v;
        }
        return this.properties.getNodeProperties(nodeId);
    }
    getEdgeProperties(key) {
        const enc = encodeTripleKey(key);
        for (let i = this.txStack.length - 1; i >= 0; i -= 1) {
            const v = this.txStack[i].edgeProps.get(enc);
            if (v !== undefined)
                return v;
        }
        return this.properties.getEdgeProperties(key);
    }
    query(criteria) {
        const now = Date.now();
        if (this.pinnedEpochStack.length === 0 && now - this.lastManifestCheck > 1000) {
            void this.refreshReadersIfEpochAdvanced();
            this.lastManifestCheck = now;
        }
        // 空条件查询：返回主存中的全部三元组（并过滤 tombstones）
        const noKeys = criteria.subjectId === undefined &&
            criteria.predicateId === undefined &&
            criteria.objectId === undefined;
        if (noKeys) {
            return this.triples.list().filter((t) => !this.tombstones.has(encodeTripleKey(t)));
        }
        const order = getBestIndexKey(criteria);
        const reader = this.pagedReaders.get(order);
        const primaryValue = criteria[primaryKey(order)];
        if (!this.dirty && reader && primaryValue !== undefined) {
            this.bumpHot(order, primaryValue);
            const triples = reader.readSync(primaryValue);
            return triples.filter((t) => matchCriteria(t, criteria) && !this.tombstones.has(encodeTripleKey(t)));
        }
        return this.indexes.query(criteria).filter((t) => !this.tombstones.has(encodeTripleKey(t)));
    }
    resolveRecords(triples) {
        const seen = new Set();
        const results = [];
        for (const t of triples) {
            if (this.tombstones.has(encodeTripleKey(t)))
                continue;
            const key = encodeTripleKey(t);
            if (seen.has(key))
                continue;
            seen.add(key);
            results.push(this.toFactRecord(t));
        }
        return results;
    }
    toFactRecord(triple) {
        const tripleKey = {
            subjectId: triple.subjectId,
            predicateId: triple.predicateId,
            objectId: triple.objectId,
        };
        return {
            subject: this.dictionary.getValue(triple.subjectId) ?? '',
            predicate: this.dictionary.getValue(triple.predicateId) ?? '',
            object: this.dictionary.getValue(triple.objectId) ?? '',
            subjectId: triple.subjectId,
            predicateId: triple.predicateId,
            objectId: triple.objectId,
            subjectProperties: this.properties.getNodeProperties(triple.subjectId),
            objectProperties: this.properties.getNodeProperties(triple.objectId),
            edgeProperties: this.properties.getEdgeProperties(tripleKey),
        };
    }
    async flush() {
        if (!this.dirty) {
            return;
        }
        const sections = {
            dictionary: this.dictionary.serialize(),
            triples: this.triples.serialize(),
            indexes: this.indexes.serialize(),
            properties: this.properties.serialize(),
        };
        // 崩溃注入：主文件写入前
        triggerCrash('before-main-write');
        await writeStorageFile(this.path, sections);
        this.dirty = false;
        // 增量刷新分页索引（仅写入新增的 staging）
        triggerCrash('before-page-append');
        await this.appendPagedIndexesFromStaging();
        // 将 tombstones 写入 manifest 以便重启恢复
        const manifest = (await readPagedManifest(this.indexDirectory)) ?? {
            version: 1,
            pageSize: DEFAULT_PAGE_SIZE,
            createdAt: Date.now(),
            compression: { codec: 'none' },
            lookups: [],
        };
        manifest.tombstones = [...this.tombstones]
            .map((k) => decodeTripleKey(k))
            .map((ids) => [ids.subjectId, ids.predicateId, ids.objectId]);
        triggerCrash('before-manifest-write');
        await writePagedManifest(this.indexDirectory, manifest);
        // 持久化热度计数（带半衰衰减）
        const hot = this.hotness;
        if (hot) {
            const now = Date.now();
            const halfLifeMs = 10 * 60 * 1000; // 10 分钟半衰期
            const decay = (elapsed) => {
                const k = Math.pow(0.5, elapsed / halfLifeMs);
                return k;
            };
            const elapsed = now - (hot.updatedAt ?? now);
            if (elapsed > 0) {
                Object.keys(hot.counts).forEach((order) => {
                    const bucket = hot.counts[order] ?? {};
                    const factor = decay(elapsed);
                    for (const key of Object.keys(bucket)) {
                        bucket[key] = Math.floor(bucket[key] * factor);
                        if (bucket[key] <= 0)
                            delete bucket[key];
                    }
                    hot.counts[order] = bucket;
                });
            }
            await writeHotness(this.indexDirectory, hot);
        }
        // 将 LSM-Lite 暂存写入段文件（实验性旁路，不改变查询可见性）
        await this.flushLsmSegments();
        triggerCrash('before-wal-reset');
        await this.wal.reset();
    }
    async flushLsmSegments() {
        if (!this.lsm)
            return;
        const entries = this.lsm.drain();
        if (!entries || entries.length === 0)
            return;
        try {
            const dir = join(this.indexDirectory, 'lsm');
            await fsp.mkdir(dir, { recursive: true });
            const buf = Buffer.allocUnsafe(entries.length * 12);
            let off = 0;
            for (const t of entries) {
                buf.writeUInt32LE(t.subjectId, off);
                off += 4;
                buf.writeUInt32LE(t.predicateId, off);
                off += 4;
                buf.writeUInt32LE(t.objectId, off);
                off += 4;
            }
            const crc = this.crc32(buf);
            const name = `seg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}.bin`;
            const file = join(dir, name);
            const fh = await fsp.open(file, 'w');
            try {
                await fh.write(buf, 0, buf.length, 0);
                await fh.sync();
            }
            finally {
                await fh.close();
            }
            const manPath = join(this.indexDirectory, 'lsm-manifest.json');
            let manifest;
            try {
                const m = await fsp.readFile(manPath);
                manifest = JSON.parse(m.toString('utf8'));
            }
            catch {
                manifest = { version: 1, segments: [] };
            }
            manifest.segments.push({
                file: `lsm/${name}`,
                count: entries.length,
                bytes: buf.length,
                crc32: crc,
                createdAt: Date.now(),
            });
            const tmp = `${manPath}.tmp`;
            const json = Buffer.from(JSON.stringify(manifest, null, 2), 'utf8');
            const mfh = await fsp.open(tmp, 'w');
            try {
                await mfh.write(json, 0, json.length, 0);
                await mfh.sync();
            }
            finally {
                await mfh.close();
            }
            await fsp.rename(tmp, manPath);
            try {
                const dh = await fsp.open(this.indexDirectory, 'r');
                try {
                    await dh.sync();
                }
                finally {
                    await dh.close();
                }
            }
            catch { }
        }
        catch {
            // 忽略段写入失败，不影响主流程
        }
    }
    // 轻量 CRC32（拷贝实现，便于段校验）
    // polynomial 0xEDB88320
    static CRC32_TABLE = (() => {
        const table = new Uint32Array(256);
        for (let i = 0; i < 256; i += 1) {
            let c = i;
            for (let k = 0; k < 8; k += 1) {
                c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
            }
            table[i] = c >>> 0;
        }
        return table;
    })();
    // eslint-disable-next-line class-methods-use-this
    crc32(buf) {
        let c = 0xffffffff;
        for (let i = 0; i < buf.length; i += 1) {
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            c = (PersistentStore.CRC32_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8)) >>> 0;
        }
        return (c ^ 0xffffffff) >>> 0;
    }
    async refreshReadersIfEpochAdvanced() {
        try {
            const manifest = await readPagedManifest(this.indexDirectory);
            if (!manifest)
                return;
            const epoch = manifest.epoch ?? 0;
            if (epoch > this.currentEpoch) {
                this.hydratePagedReaders(manifest);
                this.currentEpoch = epoch;
            }
        }
        catch {
            // ignore
        }
    }
    // 确保读者注册的异步锁机制
    async ensureReaderRegistered(epoch) {
        // 如果已有操作在进行中，等待其完成
        if (this.activeReaderOperation) {
            await this.activeReaderOperation;
            return;
        }
        // 如果已经注册过读者，无需重复注册
        if (this.readerRegistered) {
            return;
        }
        // 启动新的注册操作
        this.activeReaderOperation = (async () => {
            try {
                await addReader(this.indexDirectory, {
                    pid: process.pid,
                    epoch: epoch,
                    ts: Date.now(),
                });
                this.readerRegistered = true;
            }
            catch {
                // 注册失败，保持标志位为false
                this.readerRegistered = false;
            }
        })();
        try {
            await this.activeReaderOperation;
        }
        finally {
            this.activeReaderOperation = null;
        }
    }
    // 读一致性：在查询链路中临时固定 epoch，避免中途重载 readers
    async pushPinnedEpoch(epoch) {
        this.pinnedEpochStack.push(epoch);
        this.snapshotRefCount++;
        // 如果这是第一个快照，确保读者已注册
        if (this.snapshotRefCount === 1) {
            await this.ensureReaderRegistered(epoch);
        }
    }
    async popPinnedEpoch() {
        this.pinnedEpochStack.pop();
        this.snapshotRefCount--;
        // 如果这是最后一个快照，且之前注册过读者，则注销
        if (this.snapshotRefCount === 0 && this.readerRegistered) {
            try {
                await removeReader(this.indexDirectory, process.pid);
                this.readerRegistered = false;
            }
            catch {
                // 忽略注销失败，但不保证readerRegistered状态
            }
        }
    }
    getCurrentEpoch() {
        return this.currentEpoch;
    }
    // 暂存层指标（仅用于观测与基准）
    getStagingMetrics() {
        return { lsmMemtable: this.lsm ? this.lsm.size() : 0 };
    }
    async close() {
        // 释放写锁
        if (this.lock) {
            await this.lock.release();
            this.lock = undefined;
        }
        if (this.readerRegistered) {
            try {
                await removeReader(this.indexDirectory, process.pid);
            }
            catch {
                // ignore registry errors
            }
            this.readerRegistered = false;
        }
    }
    bumpHot(order, primary) {
        if (!this.hotness)
            return;
        const counts = this.hotness.counts;
        const bucket = counts[order] ?? {};
        const key = String(primary);
        bucket[key] = (bucket[key] ?? 0) + 1;
        counts[order] = bucket;
    }
    // 统一暂存写入：默认写入 TripleIndexes；在 lsm-lite 模式下旁路收集 memtable（不改变可见性）
    stageAdd(t) {
        this.indexes.add(t);
        if (this.lsm)
            this.lsm.add(t);
    }
    applyStage(stage) {
        // 应用新增
        for (const t of stage.adds) {
            if (!this.triples.has(t))
                this.triples.add(t);
            // 为查询可见性，新增统一进入暂存索引，待下一次 flush 合并分页索引
            this.stageAdd(t);
            this.dirty = true;
        }
        // 应用删除
        for (const t of stage.dels) {
            this.tombstones.add(encodeTripleKey(t));
            this.dirty = true;
        }
        // 应用属性
        stage.nodeProps.forEach((v, k) => this.setNodePropertiesDirect(k, v));
        stage.edgeProps.forEach((v, k) => {
            const ids = decodeTripleKey(k);
            this.setEdgePropertiesDirect(ids, v);
        });
    }
    peekTx() {
        return this.txStack[this.txStack.length - 1];
    }
}
function primaryKey(order) {
    return order === 'SPO' ? 'subjectId' : order === 'POS' ? 'predicateId' : 'objectId';
}
function primarySelector(order) {
    if (order === 'SPO')
        return (t) => t.subjectId;
    if (order === 'POS')
        return (t) => t.predicateId;
    return (t) => t.objectId;
}
function matchCriteria(t, criteria) {
    if (criteria.subjectId !== undefined && t.subjectId !== criteria.subjectId)
        return false;
    if (criteria.predicateId !== undefined && t.predicateId !== criteria.predicateId)
        return false;
    if (criteria.objectId !== undefined && t.objectId !== criteria.objectId)
        return false;
    return true;
}
function encodeTripleKey({ subjectId, predicateId, objectId }) {
    return `${subjectId}:${predicateId}:${objectId}`;
}
function decodeTripleKey(key) {
    const [s, p, o] = key.split(':').map((x) => Number(x));
    return { subjectId: s, predicateId: p, objectId: o };
}
//# sourceMappingURL=persistentStore.js.map