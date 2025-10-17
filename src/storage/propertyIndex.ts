/**
 * 属性倒排索引 - 支持基于属性值的高效查询
 *
 * 设计目标：
 * - 支持 O(log n) 时间复杂度的属性值查询
 * - 支持范围查询和等值查询
 * - 内存友好的分页存储
 * - 支持增量更新和批量重建
 */

import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { PropertyDataStore } from './propertyDataStore.js';
import type { TripleKey } from './propertyStore.js';

// 索引条目：属性名 -> 值 -> ID集合
export interface PropertyIndexEntry {
  propertyName: string;
  value: unknown;
  nodeIds?: Set<number>; // 节点属性索引
  edgeKeys?: Set<string>; // 边属性索引
}

// 属性索引操作类型
export type PropertyOperation = 'SET' | 'DELETE';

export interface PropertyChange {
  operation: PropertyOperation;
  target: 'node' | 'edge';
  targetId: number | string;
  propertyName: string;
  oldValue?: unknown;
  newValue?: unknown;
}

/**
 * 内存属性索引 - 暂存层，支持快速查询和更新
 */
export class MemoryPropertyIndex {
  // nodeProperties: 属性名 -> 归一化值 -> nodeId集合
  private readonly nodeProperties = new Map<
    string,
    Map<string | number | boolean | null, Set<number>>
  >();

  // edgeProperties: 属性名 -> 归一化值 -> edgeKey集合
  private readonly edgeProperties = new Map<
    string,
    Map<string | number | boolean | null, Set<string>>
  >();

  /**
   * 添加节点属性到索引
   */
  indexNodeProperty(nodeId: number, propertyName: string, value: unknown): void {
    if (!this.nodeProperties.has(propertyName)) {
      this.nodeProperties.set(propertyName, new Map());
    }

    const valueMap = this.nodeProperties.get(propertyName)!;
    const key = this.normalizeValue(value);

    if (!valueMap.has(key)) {
      valueMap.set(key, new Set());
    }

    valueMap.get(key)!.add(nodeId);
  }

  /**
   * 添加边属性到索引
   */
  indexEdgeProperty(edgeKey: string, propertyName: string, value: unknown): void {
    if (!this.edgeProperties.has(propertyName)) {
      this.edgeProperties.set(propertyName, new Map());
    }

    const valueMap = this.edgeProperties.get(propertyName)!;
    const key = this.normalizeValue(value);

    if (!valueMap.has(key)) {
      valueMap.set(key, new Set());
    }

    valueMap.get(key)!.add(edgeKey);
  }

  /**
   * 从索引中移除节点属性
   */
  removeNodeProperty(nodeId: number, propertyName: string, value: unknown): void {
    const valueMap = this.nodeProperties.get(propertyName);
    if (!valueMap) return;

    const key = this.normalizeValue(value);
    const nodeSet = valueMap.get(key);
    if (!nodeSet) return;

    nodeSet.delete(nodeId);
    if (nodeSet.size === 0) {
      valueMap.delete(key);
      if (valueMap.size === 0) {
        this.nodeProperties.delete(propertyName);
      }
    }
  }

  /**
   * 从索引中移除边属性
   */
  removeEdgeProperty(edgeKey: string, propertyName: string, value: unknown): void {
    const valueMap = this.edgeProperties.get(propertyName);
    if (!valueMap) return;

    const key = this.normalizeValue(value);
    const edgeSet = valueMap.get(key);
    if (!edgeSet) return;

    edgeSet.delete(edgeKey);
    if (edgeSet.size === 0) {
      valueMap.delete(key);
      if (valueMap.size === 0) {
        this.edgeProperties.delete(propertyName);
      }
    }
  }

  /**
   * 查询具有指定属性值的节点ID
   */
  queryNodesByProperty(propertyName: string, value: unknown): Set<number> {
    const valueMap = this.nodeProperties.get(propertyName);
    if (!valueMap) return new Set();

    const key = this.normalizeValue(value);
    return new Set(valueMap.get(key) || []);
  }

  /**
   * 查询具有指定属性值的边键
   */
  queryEdgesByProperty(propertyName: string, value: unknown): Set<string> {
    const valueMap = this.edgeProperties.get(propertyName);
    if (!valueMap) return new Set();

    const key = this.normalizeValue(value);
    return new Set(valueMap.get(key) || []);
  }

  /**
   * 范围查询节点 (用于数值比较)
   */
  queryNodesByRange(
    propertyName: string,
    min?: unknown,
    max?: unknown,
    includeMin = true,
    includeMax = true,
  ): Set<number> {
    const valueMap = this.nodeProperties.get(propertyName);
    if (!valueMap) return new Set();

    const results = new Set<number>();

    for (const [value, nodeIds] of valueMap.entries()) {
      if (this.isInRange(value, min, max, includeMin, includeMax)) {
        for (const nodeId of nodeIds) {
          results.add(nodeId);
        }
      }
    }

    return results;
  }

  /**
   * 获取所有属性名
   */
  getNodePropertyNames(): string[] {
    return Array.from(this.nodeProperties.keys());
  }

  getEdgePropertyNames(): string[] {
    return Array.from(this.edgeProperties.keys());
  }

  /**
   * 获取统计信息
   */
  getStats(): {
    nodePropertyCount: number;
    edgePropertyCount: number;
    totalNodeEntries: number;
    totalEdgeEntries: number;
  } {
    let totalNodeEntries = 0;
    let totalEdgeEntries = 0;

    for (const valueMap of this.nodeProperties.values()) {
      for (const nodeSet of valueMap.values()) {
        totalNodeEntries += nodeSet.size;
      }
    }

    for (const valueMap of this.edgeProperties.values()) {
      for (const edgeSet of valueMap.values()) {
        totalEdgeEntries += edgeSet.size;
      }
    }

    return {
      nodePropertyCount: this.nodeProperties.size,
      edgePropertyCount: this.edgeProperties.size,
      totalNodeEntries,
      totalEdgeEntries,
    };
  }

  /**
   * 清空索引
   */
  clear(): void {
    this.nodeProperties.clear();
    this.edgeProperties.clear();
  }

  /**
   * 仅用于序列化：按属性名获取节点属性的内部映射
   */
  getNodePropertyMap(
    propertyName: string,
  ): Map<string | number | boolean | null, Set<number>> | undefined {
    return this.nodeProperties.get(propertyName);
  }

  /**
   * 仅用于序列化：按属性名获取边属性的内部映射
   */
  getEdgePropertyMap(
    propertyName: string,
  ): Map<string | number | boolean | null, Set<string>> | undefined {
    return this.edgeProperties.get(propertyName);
  }

  /**
   * 标准化值用于索引键
   */
  private normalizeValue(value: unknown): string | number | boolean | null {
    if (value === null || value === undefined) {
      return null;
    }

    // 对于对象和数组，使用 JSON 序列化作为键
    if (typeof value === 'object') {
      return JSON.stringify(value);
    }

    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
      return value;
    }
    if (typeof value === 'bigint') return value.toString();
    if (typeof value === 'symbol') return value.toString();
    if (typeof value === 'function') return '[function]';
    // 其余非常规类型统一序列化
    return JSON.stringify(value);
  }

  /**
   * 检查值是否在指定范围内
   */
  private isInRange(
    value: string | number | boolean | null,
    min?: unknown,
    max?: unknown,
    includeMin = true,
    includeMax = true,
  ): boolean {
    if (min !== undefined) {
      const cmp = this.compareValues(value, min);
      if (cmp < 0 || (cmp === 0 && !includeMin)) {
        return false;
      }
    }

    if (max !== undefined) {
      const cmp = this.compareValues(value, max);
      if (cmp > 0 || (cmp === 0 && !includeMax)) {
        return false;
      }
    }

    return true;
  }

  /**
   * 比较两个值
   */
  private compareValues(a: unknown, b: unknown): number {
    if (a === b) return 0;
    if (a == null && b != null) return -1;
    if (a != null && b == null) return 1;

    // 数值比较
    if (typeof a === 'number' && typeof b === 'number') {
      return a - b;
    }

    // 字符串比较
    if (typeof a === 'string' && typeof b === 'string') {
      return a.localeCompare(b);
    }

    // 日期比较
    if (a instanceof Date && b instanceof Date) {
      return a.getTime() - b.getTime();
    }

    // 其他类型转换为字符串比较
    return String(a).localeCompare(String(b));
  }
}

// 预留：分页属性索引清单（未来持久化时使用）

/**
 * 持久化属性索引管理器
 *
 * 架构演进（v2.0）：
 * - 倒排索引（已有）：属性名->值->ID集合（用于whereProperty查询）
 * - 正向索引（新增）：ID->属性数据（用于getNodeProperties查询）
 */
export class PropertyIndexManager {
  private readonly memoryIndex = new MemoryPropertyIndex();
  private readonly indexDirectory: string;
  private readonly manifestPath: string;
  private readonly indexPaths = new Map<string, string>(); // 属性名 -> 索引文件路径
  private manifestLoaded = false;

  // 新增：正向属性数据存储（磁盘分页 + 内存缓存）
  private readonly propertyDataStore: PropertyDataStore;

  constructor(indexDirectory: string, pageSize = 1024) {
    this.indexDirectory = indexDirectory;
    this.manifestPath = path.join(indexDirectory, 'property-index.manifest.json');
    this.propertyDataStore = new PropertyDataStore(indexDirectory, pageSize);
  }

  /**
   * 获取内存索引实例
   */
  getMemoryIndex(): MemoryPropertyIndex {
    return this.memoryIndex;
  }

  /**
   * 初始化属性索引目录和数据存储
   */
  async initialize(): Promise<void> {
    try {
      await fs.mkdir(this.indexDirectory, { recursive: true });
    } catch {}

    // 初始化属性数据存储
    await this.propertyDataStore.initialize();
  }

  /**
   * 从现有属性数据重建索引
   *
   * 架构重构（Issue #7）：同时将数据迁移到 PropertyDataStore
   */
  async rebuildFromProperties(
    nodeProperties: Map<number, Record<string, unknown>>,
    edgeProperties: Map<string, Record<string, unknown>>,
  ): Promise<void> {
    // 如果已经加载了持久化索引，优先使用它
    if (!this.manifestLoaded) {
      try {
        await this.load();
        return;
      } catch {
        // 持久化索引不存在或损坏，继续内存重建
      }
    }

    this.memoryIndex.clear();

    // 重建节点属性索引
    for (const [nodeId, props] of nodeProperties.entries()) {
      // 更新倒排索引
      for (const [propName, value] of Object.entries(props)) {
        this.memoryIndex.indexNodeProperty(nodeId, propName, value);
      }

      // 架构重构（Issue #7）：同时设置到 PropertyDataStore 缓存
      this.propertyDataStore.setNodeProperties(nodeId, props);
    }

    // 重建边属性索引
    for (const [edgeKey, props] of edgeProperties.entries()) {
      // 更新倒排索引
      for (const [propName, value] of Object.entries(props)) {
        this.memoryIndex.indexEdgeProperty(edgeKey, propName, value);
      }

      // 架构重构（Issue #7）：同时设置到 PropertyDataStore 缓存
      this.propertyDataStore.setEdgeProperties(edgeKey, props);
    }
  }

  /**
   * 处理属性变更
   */
  applyPropertyChange(change: PropertyChange): void {
    if (change.target === 'node') {
      const nodeId = change.targetId as number;

      if (change.operation === 'DELETE' && change.oldValue !== undefined) {
        this.memoryIndex.removeNodeProperty(nodeId, change.propertyName, change.oldValue);
      } else if (change.operation === 'SET') {
        // 先删除旧值（如果存在）
        if (change.oldValue !== undefined) {
          this.memoryIndex.removeNodeProperty(nodeId, change.propertyName, change.oldValue);
        }
        // 添加新值
        if (change.newValue !== undefined) {
          this.memoryIndex.indexNodeProperty(nodeId, change.propertyName, change.newValue);
        }
      }
    } else if (change.target === 'edge') {
      const edgeKey = change.targetId as string;

      if (change.operation === 'DELETE' && change.oldValue !== undefined) {
        this.memoryIndex.removeEdgeProperty(edgeKey, change.propertyName, change.oldValue);
      } else if (change.operation === 'SET') {
        // 先删除旧值（如果存在）
        if (change.oldValue !== undefined) {
          this.memoryIndex.removeEdgeProperty(edgeKey, change.propertyName, change.oldValue);
        }
        // 添加新值
        if (change.newValue !== undefined) {
          this.memoryIndex.indexEdgeProperty(edgeKey, change.propertyName, change.newValue);
        }
      }
    }
  }

  /**
   * 持久化索引到磁盘（包括倒排索引和正向数据）
   */
  async flush(): Promise<void> {
    try {
      await fs.mkdir(this.indexDirectory, { recursive: true });
    } catch {}

    // 1. 持久化倒排索引（现有逻辑）
    const indexData = this.serializeIndex();

    // 写入索引文件
    const manifest = {
      version: 1,
      timestamp: Date.now(),
      nodePropertyNames: this.memoryIndex.getNodePropertyNames(),
      edgePropertyNames: this.memoryIndex.getEdgePropertyNames(),
      stats: this.memoryIndex.getStats(),
      files: [] as string[],
    };

    // 为每个属性名创建独立的索引文件
    for (const [propertyName, data] of indexData.entries()) {
      const fileName = `property-${Buffer.from(propertyName).toString('base64url')}.idx`;
      const filePath = path.join(this.indexDirectory, fileName);
      await fs.writeFile(filePath, data);
      manifest.files.push(fileName);
      this.indexPaths.set(propertyName, filePath);
    }

    // 写入清单文件
    await fs.writeFile(this.manifestPath, JSON.stringify(manifest, null, 2));

    // 2. 持久化正向属性数据（新增）
    await this.propertyDataStore.flush();
  }

  /**
   * 正向查询：通过节点ID获取属性（同步，从缓存）
   */
  getNodePropertiesSync(nodeId: number): Record<string, unknown> | undefined {
    return this.propertyDataStore.getNodePropertiesSync(nodeId);
  }

  /**
   * 正向查询：通过边键获取属性（同步，从缓存）
   */
  getEdgePropertiesSync(key: TripleKey): Record<string, unknown> | undefined {
    const edgeKey = encodeTripleKey(key);
    return this.propertyDataStore.getEdgePropertiesSync(edgeKey);
  }

  /**
   * 正向查询：通过节点ID获取属性（异步，支持磁盘加载）
   */
  async getNodeProperties(nodeId: number): Promise<Record<string, unknown> | undefined> {
    return this.propertyDataStore.getNodeProperties(nodeId);
  }

  /**
   * 正向查询：通过边键获取属性（异步，支持磁盘加载）
   */
  async getEdgeProperties(key: TripleKey): Promise<Record<string, unknown> | undefined> {
    const edgeKey = encodeTripleKey(key);
    return this.propertyDataStore.getEdgeProperties(edgeKey);
  }

  /**
   * 设置节点属性（更新缓存和索引）
   */
  setNodeProperties(nodeId: number, properties: Record<string, unknown>): void {
    // 更新正向数据存储
    this.propertyDataStore.setNodeProperties(nodeId, properties);

    // 更新倒排索引（现有逻辑，通过PropertyChange）
    // 注意：调用者需要通过applyPropertyChange更新倒排索引
  }

  /**
   * 设置边属性（更新缓存和索引）
   */
  setEdgeProperties(key: TripleKey, properties: Record<string, unknown>): void {
    const edgeKey = encodeTripleKey(key);
    // 更新正向数据存储
    this.propertyDataStore.setEdgeProperties(edgeKey, properties);

    // 更新倒排索引（通过PropertyChange）
  }

  /**
   * 获取所有缓存的属性数据（用于重建索引等场景）
   */
  getAllCachedProperties(): {
    nodeProperties: Map<number, Record<string, unknown>>;
    edgeProperties: Map<string, Record<string, unknown>>;
  } {
    return {
      nodeProperties: this.propertyDataStore.getAllCachedNodeProperties(),
      edgeProperties: this.propertyDataStore.getAllCachedEdgeProperties(),
    };
  }

  /**
   * 从磁盘加载索引
   */
  async load(): Promise<void> {
    try {
      // 读取清单文件
      const manifestContent = await fs.readFile(this.manifestPath, 'utf-8');
      interface PropertyManifest {
        version: number;
        timestamp: number;
        nodePropertyNames: string[];
        edgePropertyNames: string[];
        stats: unknown;
        files: string[];
      }
      const manifest = JSON.parse(manifestContent) as PropertyManifest;

      // 清空内存索引
      this.memoryIndex.clear();

      // 加载各个索引文件
      for (const fileName of manifest.files) {
        const filePath = path.join(this.indexDirectory, fileName);
        const data = await fs.readFile(filePath);

        // 从文件名还原属性名
        const propertyName = Buffer.from(
          fileName.replace('property-', '').replace('.idx', ''),
          'base64url',
        ).toString('utf-8');
        this.indexPaths.set(propertyName, filePath);

        // 反序列化索引数据
        this.deserializeIndex(propertyName, data);
      }

      this.manifestLoaded = true;
    } catch (error) {
      // 文件不存在或损坏，标记为未加载
      this.manifestLoaded = false;
      throw new Error(
        `属性索引加载失败: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * 序列化索引数据
   */
  private serializeIndex(): Map<string, Buffer> {
    const result = new Map<string, Buffer>();

    // 序列化节点属性索引
    const nodeProperties = this.memoryIndex.getNodePropertyNames();
    for (const propName of nodeProperties) {
      const data = this.serializePropertyIndex(propName, 'node');
      if (data.length > 0) {
        result.set(`node:${propName}`, data);
      }
    }

    // 序列化边属性索引
    const edgeProperties = this.memoryIndex.getEdgePropertyNames();
    for (const propName of edgeProperties) {
      const data = this.serializePropertyIndex(propName, 'edge');
      if (data.length > 0) {
        result.set(`edge:${propName}`, data);
      }
    }

    return result;
  }

  /**
   * 序列化单个属性索引
   */
  private serializePropertyIndex(propertyName: string, type: 'node' | 'edge'): Buffer {
    const buffers: Buffer[] = [];

    // 头部：类型(1字节) + 属性名长度(4字节) + 属性名
    const typeByte = type === 'node' ? 1 : 2;
    const nameBuffer = Buffer.from(propertyName, 'utf-8');
    const header = Buffer.allocUnsafe(5);
    header.writeUInt8(typeByte, 0);
    header.writeUInt32LE(nameBuffer.length, 1);
    buffers.push(header, nameBuffer);

    // 获取属性值映射
    const valueMap =
      type === 'node'
        ? this.getPrivateNodePropertyMap(propertyName)
        : this.getPrivateEdgePropertyMap(propertyName);

    if (!valueMap) return Buffer.concat([]);

    // 写入值条目数量
    const countBuffer = Buffer.allocUnsafe(4);
    countBuffer.writeUInt32LE(valueMap.size, 0);
    buffers.push(countBuffer);

    // 序列化每个值及其对应的ID集合
    for (const [value, idSet] of valueMap.entries()) {
      // 序列化值
      const valueBuffer = this.serializeValue(value);
      const valueLength = Buffer.allocUnsafe(4);
      valueLength.writeUInt32LE(valueBuffer.length, 0);
      buffers.push(valueLength, valueBuffer);

      // 序列化ID集合
      const idArray = Array.from(idSet as unknown as Array<string | number>).sort((a, b) => {
        if (typeof a === 'string' && typeof b === 'string') {
          return a.localeCompare(b);
        }
        return Number(a) - Number(b);
      });
      const count = Buffer.allocUnsafe(4);
      count.writeUInt32LE(idArray.length, 0);
      buffers.push(count);

      if (type === 'node') {
        // 节点ID数组
        for (const id of idArray) {
          const idBuffer = Buffer.allocUnsafe(4);
          idBuffer.writeUInt32LE(Number(id), 0);
          buffers.push(idBuffer);
        }
      } else {
        // 边键数组（字符串）
        for (const key of idArray) {
          const keyBuffer = Buffer.from(key as string, 'utf-8');
          const keyLength = Buffer.allocUnsafe(4);
          keyLength.writeUInt32LE(keyBuffer.length, 0);
          buffers.push(keyLength, keyBuffer);
        }
      }
    }

    return Buffer.concat(buffers);
  }

  /**
   * 反序列化索引数据
   */
  private deserializeIndex(propertyName: string, data: Buffer): void {
    let offset = 0;

    const readUInt32 = (): number => {
      const value = data.readUInt32LE(offset);
      offset += 4;
      return value;
    };

    // 读取类型
    const typeByte = data.readUInt8(offset);
    offset += 1;
    const type = typeByte === 1 ? 'node' : 'edge';

    // 读取属性名长度和属性名
    const nameLength = readUInt32();
    const nameBuffer = data.subarray(offset, offset + nameLength);
    offset += nameLength;
    const actualPropertyName = nameBuffer.toString('utf-8');

    // 读取值条目数量
    const entryCount = readUInt32();

    for (let i = 0; i < entryCount; i++) {
      // 读取值
      const valueLength = readUInt32();
      const valueBuffer = data.subarray(offset, offset + valueLength);
      offset += valueLength;
      const value = this.deserializeValue(valueBuffer);

      // 读取ID集合数量
      const idCount = readUInt32();

      if (type === 'node') {
        // 读取节点ID
        for (let j = 0; j < idCount; j++) {
          const nodeId = readUInt32();
          this.memoryIndex.indexNodeProperty(nodeId, actualPropertyName, value);
        }
      } else {
        // 读取边键
        for (let j = 0; j < idCount; j++) {
          const keyLength = readUInt32();
          const keyBuffer = data.subarray(offset, offset + keyLength);
          offset += keyLength;
          const edgeKey = keyBuffer.toString('utf-8');
          this.memoryIndex.indexEdgeProperty(edgeKey, actualPropertyName, value);
        }
      }
    }
  }

  /**
   * 序列化值
   */
  private serializeValue(value: string | number | boolean | null): Buffer {
    if (value === null) {
      return Buffer.from([0]);
    }

    switch (typeof value) {
      case 'string': {
        const strBuf = Buffer.from(value, 'utf-8');
        const strHeader = Buffer.allocUnsafe(5);
        strHeader.writeUInt8(1, 0); // 类型：字符串
        strHeader.writeUInt32LE(strBuf.length, 1);
        return Buffer.concat([strHeader, strBuf]);
      }

      case 'number': {
        const numHeader = Buffer.allocUnsafe(9);
        numHeader.writeUInt8(2, 0); // 类型：数字
        numHeader.writeDoubleLE(value, 1);
        return numHeader;
      }

      case 'boolean': {
        const boolHeader = Buffer.allocUnsafe(2);
        boolHeader.writeUInt8(3, 0); // 类型：布尔
        boolHeader.writeUInt8(value ? 1 : 0, 1);
        return boolHeader;
      }

      default: {
        // 对象或其他类型，用JSON序列化
        const jsonStr = JSON.stringify(value);
        const jsonBuf = Buffer.from(jsonStr, 'utf-8');
        const jsonHeader = Buffer.allocUnsafe(5);
        jsonHeader.writeUInt8(4, 0); // 类型：JSON
        jsonHeader.writeUInt32LE(jsonBuf.length, 1);
        return Buffer.concat([jsonHeader, jsonBuf]);
      }
    }
  }

  /**
   * 反序列化值
   */
  private deserializeValue(buffer: Buffer): unknown {
    if (buffer.length === 0) return null;

    const type = buffer.readUInt8(0);

    switch (type) {
      case 0: // null
        return null;

      case 1: {
        // 字符串
        const strLength = buffer.readUInt32LE(1);
        return buffer.subarray(5, 5 + strLength).toString('utf-8');
      }

      case 2: {
        // 数字
        return buffer.readDoubleLE(1);
      }

      case 3: {
        // 布尔
        return buffer.readUInt8(1) === 1;
      }

      case 4: {
        // JSON
        const jsonLength = buffer.readUInt32LE(1);
        const jsonStr = buffer.subarray(5, 5 + jsonLength).toString('utf-8');
        return JSON.parse(jsonStr) as unknown;
      }

      default:
        return null;
    }
  }

  /**
   * 获取私有节点属性映射（用于序列化）
   */
  private getPrivateNodePropertyMap(
    propertyName: string,
  ): Map<string | number | boolean | null, Set<number>> | undefined {
    return this.memoryIndex.getNodePropertyMap(propertyName);
  }

  /**
   * 获取私有边属性映射（用于序列化）
   */
  private getPrivateEdgePropertyMap(
    propertyName: string,
  ): Map<string | number | boolean | null, Set<string>> | undefined {
    return this.memoryIndex.getEdgePropertyMap(propertyName);
  }
}

// 辅助函数：编码TripleKey为字符串
function encodeTripleKey({ subjectId, predicateId, objectId }: TripleKey): string {
  return `${subjectId}:${predicateId}:${objectId}`;
}
