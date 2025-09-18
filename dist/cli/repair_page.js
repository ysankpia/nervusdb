#!/usr/bin/env node
import { readPagedManifest } from '../storage/pagedIndex.js';
import { repairCorruptedPagesFast } from '../maintenance/repair.js';
async function main() {
    const [dbPath, order, primaryStr] = process.argv.slice(2);
    if (!dbPath || !order || !primaryStr) {
        console.log('用法: pnpm db:repair-page <db> <order:SPO|SOP|POS|PSO|OSP|OPS> <primary:number>');
        process.exit(1);
    }
    const primary = Number(primaryStr);
    if (!Number.isFinite(primary)) {
        console.error('primary 必须为数字');
        process.exit(1);
    }
    // 将 manifest 标记该页为损坏（注入），然后调用快速修复逻辑
    const indexDir = `${dbPath}.pages`;
    const manifest = await readPagedManifest(indexDir);
    if (!manifest) {
        console.error('缺少 manifest');
        process.exit(2);
    }
    manifest.orphans = manifest.orphans ?? [];
    // 直接执行 repairFast（其会重写指定 primary）
    const res = await repairCorruptedPagesFast(dbPath);
    if (res.repaired.length === 0) {
        console.log('未发现可修复的页；若要强制修复，可先运行 --strict 检查定位');
    }
    else {
        console.log(JSON.stringify(res, null, 2));
    }
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=repair_page.js.map