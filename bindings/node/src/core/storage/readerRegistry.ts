/**
 * Reader Registry - 基于文件系统的实现
 *
 * 借鉴LSM-Tree分层思想：每个进程管理独立的reader文件，
 * 彻底避免多进程竞争单一文件的竞态条件。
 */

import { promises as fs } from 'node:fs';
import { join } from 'node:path';

export interface ReaderInfo {
  pid: number;
  epoch: number;
  ts: number;
}

export interface ReaderRegistry {
  version: number;
  readers: ReaderInfo[];
}

const READERS_DIR = 'readers';

/**
 * 确保readers目录存在
 */
async function ensureReadersDir(directory: string): Promise<string> {
  const readersPath = join(directory, READERS_DIR);
  await fs.mkdir(readersPath, { recursive: true });
  return readersPath;
}

/**
 * 生成reader文件名：{pid}-{timestamp}.reader
 */
function getReaderFileName(pid: number, timestamp: number): string {
  return `${pid}-${timestamp}.reader`;
}

/**
 * 解析reader文件名获取pid和timestamp
 */
function parseReaderFileName(filename: string): { pid: number; timestamp: number } | null {
  const match = filename.match(/^(\d+)-(\d+)\.reader$/);
  if (!match) return null;
  return {
    pid: parseInt(match[1], 10),
    timestamp: parseInt(match[2], 10),
  };
}

/**
 * 添加reader到注册表
 * 为当前进程创建独立的reader文件
 */
export async function addReader(directory: string, info: ReaderInfo): Promise<void> {
  const readersPath = await ensureReadersDir(directory);
  const filename = getReaderFileName(info.pid, info.ts);
  const filePath = join(readersPath, filename);

  // 原子性写入：先写临时文件，再rename
  const tempPath = `${filePath}.tmp`;
  const content = JSON.stringify(info, null, 2);

  await fs.writeFile(tempPath, content, 'utf8');
  await fs.rename(tempPath, filePath);
}

/**
 * 从注册表移除reader
 * 删除当前进程的reader文件
 */
export async function removeReader(directory: string, pid: number): Promise<void> {
  const readersPath = await ensureReadersDir(directory);

  try {
    const files = await fs.readdir(readersPath);

    // 查找并删除属于指定pid的所有reader文件
    for (const file of files) {
      const parsed = parseReaderFileName(file);
      if (parsed && parsed.pid === pid) {
        const filePath = join(readersPath, file);
        try {
          await fs.unlink(filePath);
        } catch {
          // 忽略文件不存在的错误
        }
      }
    }
  } catch {
    // 如果readers目录不存在，忽略错误
  }
}

/**
 * 获取活跃的readers
 * 遍历readers目录，读取所有reader文件
 */
export async function getActiveReaders(directory: string): Promise<ReaderInfo[]> {
  try {
    const readersPath = await ensureReadersDir(directory);
    const files = await fs.readdir(readersPath);
    const readers: ReaderInfo[] = [];
    const now = Date.now();
    const staleThreshold = 30000; // 30秒过期阈值

    for (const file of files) {
      if (!file.endsWith('.reader')) continue;

      const filePath = join(readersPath, file);
      try {
        // 检查文件年龄，清理过期文件
        const stats = await fs.stat(filePath);
        const fileAge = now - stats.mtime.getTime();

        if (fileAge > staleThreshold) {
          // 文件过期，清理
          try {
            await fs.unlink(filePath);
          } catch {
            // 忽略删除失败
          }
          continue;
        }

        // 读取reader信息
        const content = await fs.readFile(filePath, 'utf8');
        const readerInfo = JSON.parse(content) as unknown as ReaderInfo;
        readers.push(readerInfo);
      } catch {
        // 忽略无效文件，继续处理其他文件
      }
    }

    return readers;
  } catch {
    // 如果目录不存在或其他错误，返回空数组
    return [];
  }
}

/**
 * 清理指定进程的所有reader文件
 * 用于进程启动时清理可能残留的旧文件
 */
export async function cleanupProcessReaders(directory: string, pid: number): Promise<void> {
  await removeReader(directory, pid);
}

/**
 * 获取当前所有活跃读者使用的 epoch 集合
 */
export async function getActiveEpochs(directory: string): Promise<number[]> {
  const readers = await getActiveReaders(directory);
  const epochs = new Set<number>();
  for (const reader of readers) {
    epochs.add(reader.epoch);
  }
  return Array.from(epochs).sort((a, b) => a - b);
}

/**
 * 检查指定 epoch 是否正在被读者使用
 */
export async function isEpochInUse(directory: string, epoch: number): Promise<boolean> {
  const readers = await getActiveReaders(directory);
  return readers.some((reader) => reader.epoch === epoch);
}

/**
 * 清理所有过期的reader文件
 * 用于维护操作
 */
export async function cleanupStaleReaders(
  directory: string,
  maxAge: number = 30000,
): Promise<void> {
  try {
    const readersPath = await ensureReadersDir(directory);
    const files = await fs.readdir(readersPath);
    const now = Date.now();

    for (const file of files) {
      if (!file.endsWith('.reader')) continue;

      const filePath = join(readersPath, file);
      try {
        const stats = await fs.stat(filePath);
        const fileAge = now - stats.mtime.getTime();

        if (fileAge > maxAge) {
          await fs.unlink(filePath);
        }
      } catch {
        // 忽略错误，继续处理其他文件
      }
    }
  } catch {
    // 忽略目录不存在等错误
  }
}

// 向后兼容：保留原有的readRegistry函数
export async function readRegistry(directory: string): Promise<ReaderRegistry> {
  const readers = await getActiveReaders(directory);
  return { version: 1, readers };
}
