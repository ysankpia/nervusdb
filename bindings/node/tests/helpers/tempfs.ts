/**
 * 测试临时目录助手
 *
 * 统一在操作系统临时目录中创建隔离的工作区（workspace），
 * 并在测试结束后递归清理，避免在仓库根目录留下任何临时文件。
 */

import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

/**
 * 创建临时工作目录。
 * @param prefix 目录前缀（会自动加上 `synapsedb-` 与尾部连字符）
 * @returns 创建好的工作区绝对路径
 */
export async function makeWorkspace(prefix: string): Promise<string> {
  const safe = prefix.replace(/[^a-zA-Z0-9_-]/g, '').toLowerCase();
  const dir = await mkdtemp(join(tmpdir(), `synapsedb-${safe}-`));
  return dir;
}

/**
 * 递归删除工作目录（包含 .wal/.pages 等衍生文件/目录）。
 * @param dir 工作区绝对路径
 */
export async function cleanupWorkspace(dir: string): Promise<void> {
  try {
    await rm(dir, { recursive: true, force: true });
  } catch {
    // 忽略清理错误（例如目录已不存在）
  }
}

/**
 * 在工作区内拼接文件/目录路径。
 */
export function within(dir: string, ...paths: string[]): string {
  return join(dir, ...paths);
}
