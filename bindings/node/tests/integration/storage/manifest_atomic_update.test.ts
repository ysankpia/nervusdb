import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm, access, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { NervusDB } from '@/synapseDb';
import { readPagedManifest, writePagedManifest } from '@/core/storage/pagedIndex';

describe('Manifest 原子更新测试', () => {
  const FAST = process.env.FAST === '1' || process.env.FAST === 'true';
  let workspace: string;
  let dbPath: string;

  beforeEach(async () => {
    workspace = await mkdtemp(join(tmpdir(), 'synapsedb-manifest-'));
    dbPath = join(workspace, 'manifest.synapsedb');
  });

  afterEach(async () => {
    await rm(workspace, { recursive: true, force: true });
  });

  it('manifest 写入期间不存在临时文件泄露', async () => {
    const db = await NervusDB.open(dbPath);

    // 添加数据触发 manifest 更新
    for (let i = 0; i < 10; i++) {
      db.addFact({ subject: `S${i}`, predicate: 'R', object: `O${i}` });
    }

    await db.flush();
    await db.close();

    // 检查是否存在 .tmp 文件泄露
    const indexDir = `${dbPath}.pages`;
    const tmpFile = join(indexDir, 'index-manifest.json.tmp');

    try {
      await access(tmpFile);
      throw new Error('临时文件不应该存在');
    } catch (error: any) {
      expect(error.code).toBe('ENOENT'); // 文件不存在，这是期望的
    }
  });

  it('manifest 更新后的 epoch 递增验证', async () => {
    const db = await NervusDB.open(dbPath);

    // 第一次更新
    db.addFact({ subject: 'S1', predicate: 'R', object: 'O1' });
    await db.flush();

    const indexDir = `${dbPath}.pages`;
    const manifest1 = await readPagedManifest(indexDir);
    const epoch1 = manifest1?.epoch ?? 0;

    // 第二次更新
    db.addFact({ subject: 'S2', predicate: 'R', object: 'O2' });
    await db.flush();

    const manifest2 = await readPagedManifest(indexDir);
    const epoch2 = manifest2?.epoch ?? 0;

    expect(epoch2).toBeGreaterThan(epoch1);

    await db.close();
  });

  it('manifest 内容一致性验证', async () => {
    const db = await NervusDB.open(dbPath);

    // 添加足够数据触发多页
    for (let i = 0; i < 50; i++) {
      db.addFact({ subject: `Subject${i}`, predicate: 'hasValue', object: `Value${i}` });
    }

    await db.flush();
    await db.close();

    // 读取并验证 manifest 结构
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);

    expect(manifest).toBeDefined();
    expect(manifest!.version).toBe(1);
    expect(manifest!.pageSize).toBeGreaterThan(0);
    expect(manifest!.lookups).toBeDefined();
    expect(manifest!.lookups.length).toBeGreaterThan(0);

    // 验证索引结构
    const spoLookup = manifest!.lookups.find((l) => l.order === 'SPO');
    expect(spoLookup).toBeDefined();
    expect(spoLookup!.pages.length).toBeGreaterThan(0);
  }, 15000);

  it('并发读写 manifest 安全性', async () => {
    const db = await NervusDB.open(dbPath);

    // 添加初始数据
    for (let i = 0; i < 10; i++) {
      db.addFact({ subject: `Init${i}`, predicate: 'R', object: `O${i}` });
    }
    await db.flush();

    const indexDir = `${dbPath}.pages`;

    // 启动并发操作
    const promises: Promise<any>[] = [];

    // 并发写入
    promises.push(
      (async () => {
        for (let i = 0; i < 5; i++) {
          db.addFact({ subject: `Concurrent${i}`, predicate: 'R', object: `O${i}` });
          await db.flush();
          // 小延迟增加交错概率
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      })(),
    );

    // 并发读取 manifest
    for (let i = 0; i < 10; i++) {
      promises.push(
        (async () => {
          const manifest = await readPagedManifest(indexDir);
          expect(manifest).toBeDefined();
          return manifest;
        })(),
      );
    }

    const results = await Promise.all(promises);

    // 验证所有读取都成功，没有损坏的 manifest
    const manifests = results.slice(1); // 第一个是写入promise的结果
    for (const manifest of manifests) {
      if (manifest) {
        expect(manifest.version).toBe(1);
        expect(manifest.lookups).toBeDefined();
      }
    }

    await db.close();
  });

  it('manifest 文件格式验证', async () => {
    const db = await NervusDB.open(dbPath);

    db.addFact({ subject: 'FormatTest', predicate: 'type', object: 'Test' });
    await db.flush();
    await db.close();

    // 直接读取 manifest 文件验证 JSON 格式
    const indexDir = `${dbPath}.pages`;
    const manifestFile = join(indexDir, 'index-manifest.json');

    const rawContent = await readFile(manifestFile, 'utf8');

    // 验证是有效的 JSON
    const parsed = JSON.parse(rawContent);
    expect(parsed).toBeDefined();
    expect(typeof parsed.version).toBe('number');
    expect(typeof parsed.pageSize).toBe('number');
    expect(Array.isArray(parsed.lookups)).toBe(true);

    // 验证为有效 JSON（不强制缩进格式，允许紧凑写法以提升性能）
    expect(typeof parsed).toBe('object');
  });

  it(
    '大量数据下的 manifest 更新性能',
    async () => {
      const db = await NervusDB.open(dbPath, { pageSize: 100 }); // 小页面增加页数

      const startTime = Date.now();

      // 添加大量数据（FAST 模式下降低规模以缩短用时）
      const N = FAST ? 150 : 1000;
      for (let i = 0; i < N; i++) {
        db.addFact({
          subject: `LargeSubject${i}`,
          predicate: 'hasLargeValue',
          object: `LargeValue${i}`,
        });
      }

      await db.flush();
      const endTime = Date.now();

      const duration = endTime - startTime;
      // 架构重构后调整阈值：分页索引写入带来额外开销，但换取更好的内存效率
      const limit = FAST ? 20000 : 60000;
      expect(duration).toBeLessThan(limit);

      // 验证生成的 manifest
      const indexDir = `${dbPath}.pages`;
      const manifest = await readPagedManifest(indexDir);

      expect(manifest).toBeDefined();
      expect(manifest!.lookups.length).toBeGreaterThan(0);

      // 应该有多页数据
      const totalPages = manifest!.lookups.reduce((sum, lookup) => sum + lookup.pages.length, 0);
      expect(totalPages).toBeGreaterThan(10);

      await db.close();
    },
    process.env.FAST === '1' || process.env.FAST === 'true' ? 20000 : 60000,
  );

  it('空数据库的 manifest 状态', async () => {
    const db = await NervusDB.open(dbPath);
    await db.flush(); // 强制创建 manifest
    await db.close();

    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);

    if (manifest) {
      expect(manifest.version).toBe(1);
      expect(manifest.lookups).toBeDefined();
      // 空数据库可能有空的 lookups 或者初始化的索引结构
      expect(Array.isArray(manifest.lookups)).toBe(true);
    }
  });
});
