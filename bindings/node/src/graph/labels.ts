import { promises as fsp } from 'node:fs';
import { dirname, join } from 'node:path';

/**
 * 节点标签系统 - Neo4j 风格的节点分类与查询
 *
 * 设计目标：
 * - 支持多标签：一个节点可以有多个标签
 * - 高效查询：按标签快速查找节点
 * - 标签组合：支持标签的交集、并集查询
 * - 持久化：标签信息随数据库持久化
 */

/**
 * 标签索引接口
 */
export interface LabelIndex {
  // 标签 -> 节点ID集合
  labelToNodes: Map<string, Set<number>>;
  // 节点ID -> 标签集合
  nodeToLabels: Map<number, Set<string>>;
}

/**
 * 带标签的节点定义
 */
export interface NodeWithLabels {
  labels?: string[];
  // 节点属性使用 unknown，避免 any 带来的不安全访问
  properties?: Record<string, unknown>;
}

/**
 * 标签查询选项
 */
export interface LabelQueryOptions {
  // 查询模式：交集（AND）或并集（OR）
  mode?: 'AND' | 'OR';
  // 限制结果数量
  limit?: number;
}

/**
 * 内存标签索引管理器
 */
export class MemoryLabelIndex {
  private readonly labelToNodes = new Map<string, Set<number>>();
  private readonly nodeToLabels = new Map<number, Set<string>>();

  /**
   * 为节点添加标签
   */
  addNodeLabels(nodeId: number, labels: string[]): void {
    if (!labels || labels.length === 0) return;

    // 去重并排序
    const uniqueLabels = [...new Set(labels)].sort();

    // 更新节点到标签的映射
    if (!this.nodeToLabels.has(nodeId)) {
      this.nodeToLabels.set(nodeId, new Set());
    }
    const nodeLabelSet = this.nodeToLabels.get(nodeId)!;

    // 更新标签到节点的映射
    for (const label of uniqueLabels) {
      nodeLabelSet.add(label);

      if (!this.labelToNodes.has(label)) {
        this.labelToNodes.set(label, new Set());
      }
      this.labelToNodes.get(label)!.add(nodeId);
    }
  }

  /**
   * 从节点移除标签
   */
  removeNodeLabels(nodeId: number, labels: string[]): void {
    const nodeLabelSet = this.nodeToLabels.get(nodeId);
    if (!nodeLabelSet) return;

    for (const label of labels) {
      nodeLabelSet.delete(label);

      // 从标签到节点的映射中移除
      const labelNodeSet = this.labelToNodes.get(label);
      if (labelNodeSet) {
        labelNodeSet.delete(nodeId);
        // 如果标签没有节点了，删除该标签
        if (labelNodeSet.size === 0) {
          this.labelToNodes.delete(label);
        }
      }
    }

    // 如果节点没有标签了，删除该节点
    if (nodeLabelSet.size === 0) {
      this.nodeToLabels.delete(nodeId);
    }
  }

  /**
   * 替换节点的所有标签
   */
  setNodeLabels(nodeId: number, labels: string[]): void {
    // 先移除所有现有标签
    const existingLabels = this.getNodeLabels(nodeId);
    if (existingLabels.length > 0) {
      this.removeNodeLabels(nodeId, existingLabels);
    }

    // 添加新标签
    this.addNodeLabels(nodeId, labels);
  }

  /**
   * 获取节点的所有标签
   */
  getNodeLabels(nodeId: number): string[] {
    const labelSet = this.nodeToLabels.get(nodeId);
    return labelSet ? [...labelSet].sort() : [];
  }

  /**
   * 查询具有指定标签的节点
   */
  findNodesByLabel(label: string): Set<number> {
    return new Set(this.labelToNodes.get(label) || []);
  }

  /**
   * 查询具有多个标签的节点（支持 AND/OR 模式）
   */
  findNodesByLabels(labels: string[], options: LabelQueryOptions = {}): Set<number> {
    if (labels.length === 0) return new Set();

    const mode = options.mode || 'AND';
    let result: Set<number>;

    if (mode === 'AND') {
      // 交集：所有标签都必须存在
      result = this.findNodesByLabel(labels[0]);
      for (let i = 1; i < labels.length; i++) {
        const labelNodes = this.findNodesByLabel(labels[i]);
        result = new Set([...result].filter((nodeId) => labelNodes.has(nodeId)));
        if (result.size === 0) break; // 提前终止
      }
    } else {
      // 并集：任一标签存在即可
      result = new Set();
      for (const label of labels) {
        const labelNodes = this.findNodesByLabel(label);
        for (const nodeId of labelNodes) {
          result.add(nodeId);
        }
      }
    }

    // 应用限制
    if (options.limit && result.size > options.limit) {
      const limited = new Set<number>();
      let count = 0;
      for (const nodeId of result) {
        if (count >= options.limit) break;
        limited.add(nodeId);
        count++;
      }
      result = limited;
    }

    return result;
  }

  /**
   * 获取所有标签
   */
  getAllLabels(): string[] {
    return [...this.labelToNodes.keys()].sort();
  }

  /**
   * 获取标签统计信息
   */
  getLabelStats(): { label: string; nodeCount: number }[] {
    const stats: { label: string; nodeCount: number }[] = [];
    for (const [label, nodeSet] of this.labelToNodes.entries()) {
      stats.push({ label, nodeCount: nodeSet.size });
    }
    return stats.sort((a, b) => b.nodeCount - a.nodeCount); // 按节点数量降序
  }

  /**
   * 检查节点是否有指定标签
   */
  hasNodeLabel(nodeId: number, label: string): boolean {
    const nodeLabelSet = this.nodeToLabels.get(nodeId);
    return nodeLabelSet ? nodeLabelSet.has(label) : false;
  }

  /**
   * 检查节点是否有任一指定标签
   */
  hasAnyNodeLabel(nodeId: number, labels: string[]): boolean {
    const nodeLabelSet = this.nodeToLabels.get(nodeId);
    if (!nodeLabelSet) return false;
    return labels.some((label) => nodeLabelSet.has(label));
  }

  /**
   * 检查节点是否有所有指定标签
   */
  hasAllNodeLabels(nodeId: number, labels: string[]): boolean {
    const nodeLabelSet = this.nodeToLabels.get(nodeId);
    if (!nodeLabelSet) return false;
    return labels.every((label) => nodeLabelSet.has(label));
  }

  /**
   * 清空所有标签索引
   */
  clear(): void {
    this.labelToNodes.clear();
    this.nodeToLabels.clear();
  }

  toSnapshot(): LabelSnapshot {
    const labelToNodes: Record<string, number[]> = {};
    for (const [label, nodes] of this.labelToNodes.entries()) {
      labelToNodes[label] = [...nodes].sort((a, b) => a - b);
    }

    const nodeToLabels: Record<string, string[]> = {};
    for (const [nodeId, labels] of this.nodeToLabels.entries()) {
      nodeToLabels[String(nodeId)] = [...labels].sort();
    }

    return {
      version: 1,
      labelToNodes,
      nodeToLabels,
    };
  }

  hydrate(snapshot: LabelSnapshot): void {
    this.clear();
    for (const [label, nodes] of Object.entries(snapshot.labelToNodes)) {
      if (!Array.isArray(nodes) || nodes.length === 0) continue;
      this.labelToNodes.set(label, new Set(nodes));
    }

    for (const [nodeIdRaw, labels] of Object.entries(snapshot.nodeToLabels)) {
      if (!Array.isArray(labels) || labels.length === 0) continue;
      const nodeId = Number(nodeIdRaw);
      this.nodeToLabels.set(nodeId, new Set(labels));
    }
  }

  /**
   * 获取统计摘要
   */
  getStats(): {
    totalLabels: number;
    totalLabeledNodes: number;
    avgLabelsPerNode: number;
    avgNodesPerLabel: number;
  } {
    const totalLabels = this.labelToNodes.size;
    const totalLabeledNodes = this.nodeToLabels.size;

    let totalLabelInstances = 0;
    for (const labelSet of this.nodeToLabels.values()) {
      totalLabelInstances += labelSet.size;
    }

    return {
      totalLabels,
      totalLabeledNodes,
      avgLabelsPerNode: totalLabeledNodes > 0 ? totalLabelInstances / totalLabeledNodes : 0,
      avgNodesPerLabel: totalLabels > 0 ? totalLabelInstances / totalLabels : 0,
    };
  }
}

interface LabelSnapshot {
  version: number;
  labelToNodes: Record<string, number[]>;
  nodeToLabels: Record<string, string[]>;
}

/**
 * 持久化标签管理器
 */
export class LabelManager {
  private readonly memoryIndex = new MemoryLabelIndex();
  private readonly filePath: string;

  constructor(private readonly indexDirectory: string) {
    this.filePath = join(indexDirectory, 'labels.index.json');
  }

  /**
   * 获取内存标签索引
   */
  getMemoryIndex(): MemoryLabelIndex {
    return this.memoryIndex;
  }

  /**
   * 从现有节点属性重建标签索引
   */
  async rebuildFromNodeProperties(
    nodeProperties: Map<number, Record<string, unknown>>,
  ): Promise<void> {
    // 保持 API 异步形态（未来可能引入异步持久化）；
    // 为满足 require-await 规则，这里添加一个可优化的微任务等待。
    await Promise.resolve();
    this.memoryIndex.clear();

    for (const [nodeId, props] of nodeProperties.entries()) {
      if (props.labels && Array.isArray(props.labels)) {
        const labels = props.labels.filter((label) => typeof label === 'string');
        if (labels.length > 0) {
          this.memoryIndex.addNodeLabels(nodeId, labels);
        }
      }
    }
  }

  /**
   * 应用标签变更
   */
  applyLabelChange(nodeId: number, oldLabels: string[], newLabels: string[]): void {
    // 计算需要添加和移除的标签
    const oldSet = new Set(oldLabels);
    const newSet = new Set(newLabels);

    const toAdd = newLabels.filter((label) => !oldSet.has(label));
    const toRemove = oldLabels.filter((label) => !newSet.has(label));

    if (toRemove.length > 0) {
      this.memoryIndex.removeNodeLabels(nodeId, toRemove);
    }

    if (toAdd.length > 0) {
      this.memoryIndex.addNodeLabels(nodeId, toAdd);
    }
  }

  /**
   * 持久化标签索引
   */
  async flush(): Promise<void> {
    const snapshot = this.memoryIndex.toSnapshot();
    await this.writeSnapshot(snapshot);
  }

  /**
   * 加载标签索引
   */
  async load(): Promise<void> {
    const snapshot = await this.readSnapshot();
    if (!snapshot) {
      throw new Error('Label snapshot missing');
    }
    this.memoryIndex.hydrate(snapshot);
  }

  async tryLoad(): Promise<boolean> {
    try {
      const snapshot = await this.readSnapshot();
      if (!snapshot) return false;
      this.memoryIndex.hydrate(snapshot);
      return true;
    } catch {
      return false;
    }
  }

  private async writeSnapshot(snapshot: LabelSnapshot): Promise<void> {
    const payload = Buffer.from(JSON.stringify(snapshot, null, 2), 'utf8');
    const tmp = `${this.filePath}.tmp`;
    await fsp.mkdir(dirname(this.filePath), { recursive: true });
    const handle = await fsp.open(tmp, 'w');
    try {
      await handle.write(payload, 0, payload.length, 0);
      await handle.sync();
    } finally {
      await handle.close();
    }
    await fsp.rename(tmp, this.filePath);
    try {
      const dirHandle = await fsp.open(this.indexDirectory, 'r');
      try {
        await dirHandle.sync();
      } finally {
        await dirHandle.close();
      }
    } catch {
      // ignore directory sync failures
    }
  }

  private async readSnapshot(): Promise<LabelSnapshot | null> {
    try {
      const buf = await fsp.readFile(this.filePath);
      return JSON.parse(buf.toString('utf8')) as LabelSnapshot;
    } catch (error: unknown) {
      if (isErrno(error) && (error.code === 'ENOENT' || error.code === 'ENOTDIR')) {
        return null;
      }
      throw error;
    }
  }
}

function isErrno(error: unknown): error is NodeJS.ErrnoException {
  return (
    typeof error === 'object' && error !== null && 'code' in (error as Record<string, unknown>)
  );
}
