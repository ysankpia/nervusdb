/**
 * PropertyDataStore - 属性数据的磁盘分页存储
 *
 * 设计目标：
 * - 正向索引：ID -> 属性数据（与PropertyIndexManager的倒排索引配合）
 * - 分页存储：按需加载，内存友好
 * - 增量持久化：只写入变更的页面
 * - LRU缓存：限制内存占用，自动淘汰冷数据
 *
 * 存储格式：
 * - manifest.json: 页面元数据管理
 * - property-data.pages: 分页数据文件
 */

import { promises as fs } from 'node:fs';
import { join } from 'node:path';
import { LRUCache } from 'lru-cache';

export interface PropertyPageMeta {
  startId: number; // 页面起始ID
  endId: number; // 页面结束ID（含）
  offset: number; // 文件偏移量
  length: number; // 数据长度
}

export interface PropertyDataManifest {
  version: number;
  pageSize: number; // 每页包含的ID范围
  createdAt: number;
  updatedAt: number;
  nodePages: PropertyPageMeta[]; // 节点属性分页
  edgePages: PropertyPageMeta[]; // 边属性分页
}

interface PropertyEntry {
  id: number; // nodeId
  data: Buffer; // 序列化的属性数据
}

/**
 * 属性数据分页存储
 */
export class PropertyDataStore {
  private readonly dataDirectory: string;
  private readonly manifestPath: string;
  private readonly nodeDataPath: string;
  private readonly edgeDataPath: string;

  // LRU缓存：限制内存占用，自动淘汰最少使用的数据
  private readonly nodeCache: LRUCache<number, Buffer>;
  private readonly edgeCache: LRUCache<string, Buffer>;

  // 分页元数据
  private manifest: PropertyDataManifest | null = null;
  private readonly pageSize: number;

  constructor(dataDirectory: string, pageSize = 1024, cacheSize = 1000) {
    this.dataDirectory = dataDirectory;
    this.manifestPath = join(dataDirectory, 'property-data.manifest.json');
    this.nodeDataPath = join(dataDirectory, 'property-data.nodes.pages');
    this.edgeDataPath = join(dataDirectory, 'property-data.edges.pages');
    this.pageSize = pageSize;

    // 初始化LRU缓存
    this.nodeCache = new LRUCache<number, Buffer>({
      max: cacheSize, // 最多缓存1000个节点属性
      // 可选：设置内存限制
      // maxSize: 10 * 1024 * 1024, // 10MB
      // sizeCalculation: (value) => value.length,
    });

    this.edgeCache = new LRUCache<string, Buffer>({
      max: cacheSize / 2, // 边属性通常较少
    });
  }

  /**
   * 初始化：创建目录和manifest（按需加载策略）
   *
   * 性能优化（Issue #12）：
   * - 移除预加载逻辑，仅加载manifest元数据
   * - 实现O(1)启动时间，数据按需从磁盘加载
   */
  async initialize(): Promise<void> {
    try {
      await fs.mkdir(this.dataDirectory, { recursive: true });
    } catch {}

    // 尝试加载现有manifest（仅元数据，不加载实际数据）
    try {
      await this.loadManifest();
      // 成功：manifest已加载，数据将按需加载
    } catch {
      // manifest不存在，创建空的
      this.manifest = {
        version: 1,
        pageSize: this.pageSize,
        createdAt: Date.now(),
        updatedAt: Date.now(),
        nodePages: [],
        edgePages: [],
      };
    }
  }

  /**
   * 同步读取节点属性（仅从LRU缓存）
   *
   * 性能优化（Issue #12）：
   * - 仅从缓存读取，不触发磁盘I/O
   * - 缓存未命中返回undefined
   * - 调用者应使用异步方法getNodeProperties()触发加载
   */
  getNodePropertiesSync(nodeId: number): Record<string, unknown> | undefined {
    const cached = this.nodeCache.get(nodeId);
    if (!cached) {
      return undefined;
    }
    return this.decodePropertyData(cached);
  }

  /**
   * 异步加载节点属性（从磁盘按需加载 + LRU缓存）
   *
   * 性能优化（Issue #12）：
   * - 缓存命中：O(1)，直接返回
   * - 缓存未命中：从磁盘加载整页，缓存所有条目（提高命中率）
   */
  async getNodeProperties(nodeId: number): Promise<Record<string, unknown> | undefined> {
    // 1. 检查LRU缓存
    const cached = this.nodeCache.get(nodeId);
    if (cached) {
      return this.decodePropertyData(cached);
    }

    // 2. 从磁盘按需加载
    if (!this.manifest) {
      return undefined;
    }

    const page = this.findNodePage(nodeId);
    if (!page) {
      return undefined;
    }

    // 加载整个页面（一次I/O加载多个条目，提高缓存命中率）
    const pageData = await this.loadNodePage(page);

    // 将整页数据缓存到LRU（自动淘汰旧数据）
    for (const [id, buffer] of pageData.entries()) {
      this.nodeCache.set(id, buffer);
    }

    // 返回查询的条目
    const entry = pageData.get(nodeId);
    if (!entry) {
      return undefined;
    }

    return this.decodePropertyData(entry);
  }

  /**
   * 设置节点属性（仅更新缓存，需要flush才持久化）
   */
  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    const encoded = this.encodePropertyData(properties);
    this.nodeCache.set(nodeId, encoded);
  }

  /**
   * 同步读取边属性（仅从缓存）
   */
  getEdgePropertiesSync(edgeKey: string): Record<string, unknown> | undefined {
    const cached = this.edgeCache.get(edgeKey);
    if (!cached) {
      return undefined;
    }
    return this.decodePropertyData(cached);
  }

  /**
   * 异步加载边属性（从磁盘）
   * 注：当前仅从缓存读取，未来将支持磁盘加载
   */
  // eslint-disable-next-line @typescript-eslint/require-await
  async getEdgeProperties(edgeKey: string): Promise<Record<string, unknown> | undefined> {
    // 1. 检查缓存
    const cached = this.edgeCache.get(edgeKey);
    if (cached) {
      return this.decodePropertyData(cached);
    }

    // 2. 边属性暂时仍使用简单的全量存储（后续优化）
    return undefined;
  }

  /**
   * 设置边属性
   */
  setEdgeProperties(edgeKey: string, properties: Record<string, unknown>): void {
    const encoded = this.encodePropertyData(properties);
    this.edgeCache.set(edgeKey, encoded);
  }

  /**
   * 持久化缓存中的修改
   */
  async flush(): Promise<void> {
    if (!this.manifest) {
      await this.initialize();
    }

    // 收集所有需要写入的节点属性
    const nodesToWrite = new Map<number, Buffer>();
    for (const [nodeId, data] of this.nodeCache.entries()) {
      nodesToWrite.set(nodeId, data);
    }

    if (nodesToWrite.size > 0) {
      await this.flushNodeProperties(nodesToWrite);
    }

    // 更新manifest
    this.manifest!.updatedAt = Date.now();
    await this.saveManifest();
  }

  /**
   * 查找包含指定nodeId的页面
   */
  private findNodePage(nodeId: number): PropertyPageMeta | undefined {
    if (!this.manifest) return undefined;

    return this.manifest.nodePages.find((page) => nodeId >= page.startId && nodeId <= page.endId);
  }

  /**
   * 从磁盘加载一个节点属性页面
   */
  private async loadNodePage(page: PropertyPageMeta): Promise<Map<number, Buffer>> {
    try {
      const handle = await fs.open(this.nodeDataPath, 'r');
      try {
        const buffer = Buffer.allocUnsafe(page.length);
        await handle.read(buffer, 0, page.length, page.offset);

        return this.deserializePage(buffer);
      } finally {
        await handle.close();
      }
    } catch {
      // 文件不存在或读取失败，返回空Map
      return new Map();
    }
  }

  /**
   * 持久化节点属性到磁盘
   */
  private async flushNodeProperties(properties: Map<number, Buffer>): Promise<void> {
    // 按ID范围分页
    const pages = this.groupIntoPages(properties);

    // 确保数据文件存在
    try {
      await fs.access(this.nodeDataPath);
    } catch {
      // 文件不存在，创建空文件
      await fs.writeFile(this.nodeDataPath, Buffer.alloc(0));
    }

    // 追加写入到数据文件
    const handle = await fs.open(this.nodeDataPath, 'a');
    try {
      let currentOffset = (await handle.stat()).size;

      for (const [startId, endId, entries] of pages) {
        const pageBuffer = this.serializePage(entries);

        await handle.write(pageBuffer, 0, pageBuffer.length, currentOffset);

        // 更新manifest
        this.manifest!.nodePages.push({
          startId,
          endId,
          offset: currentOffset,
          length: pageBuffer.length,
        });

        currentOffset += pageBuffer.length;
      }

      await handle.sync();
    } finally {
      await handle.close();
    }
  }

  /**
   * 将属性按ID范围分页
   */
  private groupIntoPages(
    properties: Map<number, Buffer>,
  ): Array<[number, number, PropertyEntry[]]> {
    // 按ID排序
    const sorted = Array.from(properties.entries()).sort(([a], [b]) => a - b);

    const pages: Array<[number, number, PropertyEntry[]]> = [];
    let currentPage: PropertyEntry[] = [];
    let pageStartId = -1;
    let pageEndId = -1;

    for (const [nodeId, data] of sorted) {
      if (pageStartId === -1) {
        pageStartId = nodeId;
      }

      currentPage.push({ id: nodeId, data });
      pageEndId = nodeId;

      // 达到pageSize或连续ID中断时创建新页
      if (currentPage.length >= this.pageSize) {
        pages.push([pageStartId, pageEndId, currentPage]);
        currentPage = [];
        pageStartId = -1;
      }
    }

    // 剩余的条目
    if (currentPage.length > 0) {
      pages.push([pageStartId, pageEndId, currentPage]);
    }

    return pages;
  }

  /**
   * 序列化一个页面
   */
  private serializePage(entries: PropertyEntry[]): Buffer {
    const buffers: Buffer[] = [];

    // 头部：条目数量
    const header = Buffer.allocUnsafe(4);
    header.writeUInt32LE(entries.length, 0);
    buffers.push(header);

    for (const entry of entries) {
      // 条目格式：nodeId(4字节) + dataLength(4字节) + data
      const entryHeader = Buffer.allocUnsafe(8);
      entryHeader.writeUInt32LE(entry.id, 0);
      entryHeader.writeUInt32LE(entry.data.length, 4);
      buffers.push(entryHeader, entry.data);
    }

    return Buffer.concat(buffers);
  }

  /**
   * 反序列化一个页面
   */
  private deserializePage(buffer: Buffer): Map<number, Buffer> {
    const result = new Map<number, Buffer>();
    let offset = 0;

    const readUInt32 = (): number => {
      const value = buffer.readUInt32LE(offset);
      offset += 4;
      return value;
    };

    const entryCount = readUInt32();

    for (let i = 0; i < entryCount; i++) {
      const nodeId = readUInt32();
      const dataLength = readUInt32();
      const data = buffer.subarray(offset, offset + dataLength);
      offset += dataLength;

      result.set(nodeId, Buffer.from(data));
    }

    return result;
  }

  /**
   * 编码属性数据为Buffer
   */
  private encodePropertyData(properties: Record<string, unknown>): Buffer {
    const json = JSON.stringify({ __v: 0, data: properties });
    return Buffer.from(json, 'utf8');
  }

  /**
   * 解码属性数据
   */
  private decodePropertyData(buffer: Buffer): Record<string, unknown> {
    if (buffer.length === 0) return {};

    try {
      const parsed = JSON.parse(buffer.toString('utf-8')) as
        | Record<string, unknown>
        | { data?: unknown };

      if (parsed && typeof parsed === 'object' && 'data' in parsed) {
        return (parsed.data as Record<string, unknown>) ?? {};
      }

      return parsed as Record<string, unknown>;
    } catch {
      return {};
    }
  }

  /**
   * 保存manifest到磁盘
   */
  private async saveManifest(): Promise<void> {
    if (!this.manifest) return;

    await fs.writeFile(this.manifestPath, JSON.stringify(this.manifest, null, 2));
  }

  /**
   * 从磁盘加载manifest
   */
  private async loadManifest(): Promise<void> {
    const content = await fs.readFile(this.manifestPath, 'utf-8');
    this.manifest = JSON.parse(content) as PropertyDataManifest;
  }

  /**
   * 获取所有缓存的节点属性（用于重建索引和flush）
   */
  getAllCachedNodeProperties(): Map<number, Record<string, unknown>> {
    const result = new Map<number, Record<string, unknown>>();
    // LRUCache 支持 entries() 迭代器
    for (const [nodeId, buffer] of this.nodeCache.entries()) {
      const properties = this.decodePropertyData(buffer);
      if (properties && Object.keys(properties).length > 0) {
        result.set(nodeId, properties);
      }
    }
    return result;
  }

  /**
   * 获取所有缓存的边属性（用于重建索引和flush）
   */
  getAllCachedEdgeProperties(): Map<string, Record<string, unknown>> {
    const result = new Map<string, Record<string, unknown>>();
    for (const [edgeKey, buffer] of this.edgeCache.entries()) {
      const properties = this.decodePropertyData(buffer);
      if (properties && Object.keys(properties).length > 0) {
        result.set(edgeKey, properties);
      }
    }
    return result;
  }

  /**
   * 获取缓存统计信息
   */
  getCacheStats(): {
    nodeCacheSize: number;
    edgeCacheSize: number;
    nodeHitRate?: number;
    edgeHitRate?: number;
  } {
    return {
      nodeCacheSize: this.nodeCache.size,
      edgeCacheSize: this.edgeCache.size,
      // LRU缓存暂不提供命中率统计，可后续添加
    };
  }

  /**
   * 清空缓存（测试用）
   */
  clearCache(): void {
    this.nodeCache.clear();
    this.edgeCache.clear();
  }
}
