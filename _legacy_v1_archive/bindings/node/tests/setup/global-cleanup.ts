import { readdir, rm } from 'node:fs/promises';
import { join } from 'node:path';

// 全局清理：在测试全部结束后，移除仓库根目录下由测试创建的临时数据库
// 约定模式：tmp-*.redb 及其关联的 .wal/.lock 文件（如存在）

export default async function globalSetup() {
  // 返回 teardown，在所有测试执行完毕后运行
  return async () => {
    try {
      const cwd = process.cwd();
      const entries = await readdir(cwd).catch(() => [] as string[]);
      for (const name of entries) {
        if (!name.startsWith('tmp-') || !name.endsWith('.redb')) continue;
        const base = join(cwd, name);
        // 主文件
        await rm(base, { force: true });
        // 关联文件（如存在）
        await rm(`${base}.wal`, { force: true });
        await rm(`${base}.lock`, { force: true });
      }
    } catch {
      // 清理失败不影响测试退出
    }
  };
}
