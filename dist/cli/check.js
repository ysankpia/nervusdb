#!/usr/bin/env node
import { basename, join } from 'node:path';
import { promises as fs } from 'node:fs';
import { readStorageFile } from '../storage/fileHeader.js';
import { pageFileName, readPagedManifest, writePagedManifest } from '../storage/pagedIndex.js';
import { SynapseDB } from '../synapseDb.js';
import { checkStrict } from '../maintenance/check.js';
import { repairCorruptedOrders, repairCorruptedPagesFast } from '../maintenance/repair.js';
async function check(dbPath) {
    const errors = [];
    try {
        await readStorageFile(dbPath);
    }
    catch (e) {
        errors.push(`主文件读取失败: ${e.message}`);
        return { ok: false, errors };
    }
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    if (!manifest) {
        errors.push('缺少分页索引 manifest');
        return { ok: false, errors };
    }
    for (const lookup of manifest.lookups) {
        const file = join(indexDir, pageFileName(lookup.order));
        let handle = null;
        try {
            handle = await fs.open(file, 'r');
            for (const page of lookup.pages) {
                try {
                    const buf = Buffer.allocUnsafe(page.length);
                    await handle.read(buf, 0, page.length, page.offset);
                    // 简易 CRC 复核：pagedIndex.ts 在读路径会做更严格校验；这里仅确认切片可读
                    if (page.length <= 0) {
                        errors.push(`${lookup.order} 页长度非法: ${JSON.stringify(page)}`);
                    }
                }
                catch (e) {
                    errors.push(`${lookup.order} 页读取失败 @offset=${page.offset} length=${page.length}: ${e.message}`);
                }
            }
        }
        catch (e) {
            errors.push(`索引文件不存在或无法打开: ${basename(file)} -> ${e.message}`);
        }
        finally {
            if (handle)
                await handle.close();
        }
    }
    return { ok: errors.length === 0, errors };
}
async function repair(dbPath) {
    const indexDir = `${dbPath}.pages`;
    const prev = await readPagedManifest(indexDir);
    const db = await SynapseDB.open(dbPath, { rebuildIndexes: true });
    await db.flush();
    // 尝试保留 tombstones
    if (prev && prev.tombstones && prev.tombstones.length > 0) {
        const now = await readPagedManifest(indexDir);
        if (now) {
            now.tombstones = prev.tombstones;
            await writePagedManifest(indexDir, now);
        }
    }
}
async function main() {
    const [cmd, dbPath] = process.argv.slice(2);
    if (!cmd || !dbPath) {
        console.log('用法: pnpm db:check <db> | pnpm db:repair <db>');
        process.exit(1);
    }
    if (cmd === 'check') {
        const strict = process.argv.includes('--strict');
        const summary = process.argv.includes('--summary');
        if (strict) {
            const r = await checkStrict(dbPath);
            console.log(JSON.stringify(r, null, 2));
            process.exit(r.ok ? 0 : 2);
        }
        const r = await check(dbPath);
        if (!r.ok) {
            console.error('检查失败:');
            r.errors.forEach((e) => console.error(' -', e));
            process.exit(2);
        }
        if (summary) {
            // 简要概览：按顺序统计页数/多页 primary 数
            const indexDir = `${dbPath}.pages`;
            const manifest = await readPagedManifest(indexDir);
            const orders = {};
            if (manifest) {
                for (const l of manifest.lookups) {
                    const cnt = new Map();
                    for (const p of l.pages)
                        cnt.set(p.primaryValue, (cnt.get(p.primaryValue) ?? 0) + 1);
                    const multi = [...cnt.values()].filter((c) => c > 1).length;
                    orders[l.order] = {
                        pages: l.pages.length,
                        primaries: cnt.size,
                        multiPagePrimaries: multi,
                    };
                }
                const orphanCount = (manifest.orphans ?? []).reduce((acc, g) => acc + g.pages.length, 0);
                console.log(JSON.stringify({ ok: true, epoch: manifest.epoch ?? 0, orders, orphans: orphanCount }, null, 2));
            }
            else {
                console.log(JSON.stringify({ ok: true, orders }, null, 2));
            }
        }
        else {
            console.log('检查通过');
        }
        process.exit(0);
    }
    if (cmd === 'repair') {
        const fast = process.argv.includes('--fast');
        // 优先尝试按页级快速修复（primary 级替换映射）；如无损坏则尝试按序修复；再无则全量重建
        if (fast) {
            const fastRes = await repairCorruptedPagesFast(dbPath);
            if (fastRes.repaired.length > 0) {
                console.log(`快速修复完成：${fastRes.repaired.map((r) => `${r.order}[${r.primaryValues.join(',')}]`).join('; ')}`);
                process.exit(0);
            }
        }
        const repaired = await repairCorruptedOrders(dbPath);
        if (repaired.repairedOrders.length > 0) {
            console.log(`修复完成（按序重写）：${repaired.repairedOrders.join(', ')}`);
            process.exit(0);
        }
        // 没有损坏则执行全量重建（也可直接返回）
        await repair(dbPath);
        console.log('修复完成（全量重建，保留 tombstones）');
        process.exit(0);
    }
    console.log('未知命令:', cmd);
    process.exit(1);
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=check.js.map