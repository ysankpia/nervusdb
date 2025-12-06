import { promises as fs } from 'node:fs';

export interface LockHandle {
  release(): Promise<void>;
}

/**
 * 获取写锁，支持重试机制
 * @param basePath 数据库文件路径
 * @param maxRetries 最大重试次数（默认 3）
 * @param baseDelayMs 基础延迟毫秒数（默认 100）
 */
export async function acquireLock(
  basePath: string,
  maxRetries = 3,
  baseDelayMs = 100,
): Promise<LockHandle> {
  const lockPath = `${basePath}.lock`;
  let fh: fs.FileHandle | null = null;
  let lastError: Error | null = null;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      fh = await fs.open(lockPath, 'wx'); // fail if exists
      const payload = Buffer.from(
        JSON.stringify({ pid: process.pid, startedAt: Date.now() }, null, 2),
        'utf8',
      );
      await fh.write(payload, 0, payload.length, 0);
      await fh.sync();
      break; // 成功获取锁，跳出循环
    } catch (e) {
      lastError = e as Error;
      if (attempt < maxRetries) {
        // 指数退避：100ms, 200ms, 400ms
        const delayMs = baseDelayMs * Math.pow(2, attempt);
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
    }
  }

  if (!fh) {
    throw new Error(
      `数据库正被占用（已重试 ${maxRetries} 次）: ${lastError?.message}\n` +
        `提示：请检查是否有其他进程正在使用此数据库，或删除 ${lockPath} 文件后重试`,
    );
  }

  const release = async () => {
    try {
      await fh?.close();
    } catch {}
    try {
      await fs.unlink(lockPath);
    } catch {}
  };

  process.once('exit', () => {
    void release();
  });
  process.once('SIGINT', () => {
    void release().then(() => process.exit(130));
  });
  process.once('SIGTERM', () => {
    void release().then(() => process.exit(143));
  });

  return { release };
}
