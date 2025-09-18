import { readPagedManifest, PagedIndexReader } from '../storage/pagedIndex';
import { readStorageFile } from '../storage/fileHeader';
import { StringDictionary } from '../storage/dictionary';
async function dump(dbPath, order, primaryValue) {
    const manifest = await readPagedManifest(`${dbPath}.pages`);
    if (!manifest) {
        console.error('未找到 manifest');
        process.exit(2);
    }
    const lookup = manifest.lookups.find((l) => l.order === order);
    if (!lookup) {
        console.error('未知顺序或无页：', order);
        process.exit(2);
    }
    const reader = new PagedIndexReader({ directory: `${dbPath}.pages`, compression: manifest.compression }, lookup);
    const triples = await reader.read(primaryValue);
    // 解析字典，打印人类可读
    const sections = await readStorageFile(dbPath);
    const dict = StringDictionary.deserialize(sections.dictionary);
    const toValue = (id) => dict.getValue(id) ?? `#${id}`;
    for (const t of triples) {
        console.log(`${t.subjectId}:${t.predicateId}:${t.objectId}  // ${toValue(t.subjectId)} ${toValue(t.predicateId)} ${toValue(t.objectId)}`);
    }
}
async function main() {
    const [dbPath, order, primary] = process.argv.slice(2);
    if (!dbPath || !order || !primary) {
        console.log('用法: pnpm db:dump <db> <order:SPO|SOP|...> <primaryValue:number>');
        process.exit(1);
    }
    const pv = Number(primary);
    if (!Number.isFinite(pv)) {
        console.error('primaryValue 必须为数字');
        process.exit(1);
    }
    await dump(dbPath, order, pv);
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=dump.js.map