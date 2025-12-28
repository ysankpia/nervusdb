/**
 * 流式查询迭代器与工具函数
 *
 * 说明：
 * - 提供将批量异步迭代（FactRecord[]）扁平化为逐条异步迭代的工具。
 * - 该模块为里程碑交付清单的占位与基础能力封装，核心流式能力已在 QueryBuilder 与 PersistentStore 内实现。
 */

import type { FactRecord } from '../../core/storage/persistentStore.js';

/**
 * 将批量异步迭代器扁平化为逐条记录的异步迭代器。
 */
export async function* flattenBatches(
  batches: AsyncIterable<FactRecord[]>,
): AsyncGenerator<FactRecord, void, unknown> {
  for await (const batch of batches) {
    for (const r of batch) {
      yield r;
    }
  }
}
