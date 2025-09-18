import { promises as fs } from 'node:fs';

export interface LockHandle {
  release(): Promise<void>;
}

export async function acquireLock(basePath: string): Promise<LockHandle> {
  const lockPath = `${basePath}.lock`;
  let fh: fs.FileHandle | null = null;
  try {
    fh = await fs.open(lockPath, 'wx'); // fail if exists
    const payload = Buffer.from(JSON.stringify({ pid: process.pid, startedAt: Date.now() }, null, 2), 'utf8');
    await fh.write(payload, 0, payload.length, 0);
    await fh.sync();
  } catch (e) {
    throw new Error(`数据库正被占用（可能有写入者存在）: ${(e as Error).message}`);
  }

  const release = async () => {
    try { await fh?.close(); } catch {}
    try { await fs.unlink(lockPath); } catch {}
  };

  process.once('exit', () => { void release(); });
  process.once('SIGINT', () => { void release().then(()=>process.exit(130)); });
  process.once('SIGTERM', () => { void release().then(()=>process.exit(143)); });

  return { release };
}

