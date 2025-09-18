// 轻量写入策略接口（预留）：当前默认使用内存暂存索引 TripleIndexes
// 后续可实现 LSM-Lite 式分段写入并在 flush/compact 时合并

export interface StagingStrategy<T> {
  // 推入一条记录
  add(rec: T): void;
  // 返回当前段大小（条目）
  size(): number;
}

export type StagingMode = 'default' | 'lsm-lite';

// 极简 LSM-Lite 暂存实现（占位）：
// 当前仍然通过 TripleIndexes 提供可见性；本类仅做旁路收集与统计，便于后续换轨实现。
export class LsmLiteStaging<T> implements StagingStrategy<T> {
  private memtable: T[] = [];
  add(rec: T): void {
    this.memtable.push(rec);
  }
  size(): number {
    return this.memtable.length;
  }
  // 取出并清空当前 memtable
  drain(): T[] {
    const out = this.memtable;
    this.memtable = [];
    return out;
  }
  // 未来：支持 flush 到段文件、compact 合并
}
