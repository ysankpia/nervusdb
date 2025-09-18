import { SynapseDB } from '../synapseDb';
async function main() {
    const [dbPath, countArg] = process.argv.slice(2);
    if (!dbPath) {
        console.log('用法: pnpm bench <db> [count=10000]');
        process.exit(1);
    }
    const count = Number(countArg ?? '10000');
    const db = await SynapseDB.open(dbPath, { pageSize: 1024 });
    console.time('insert');
    for (let i = 0; i < count; i += 1) {
        db.addFact({ subject: `S${i % 1000}`, predicate: `R${i % 50}`, object: `O${i}` });
    }
    console.timeEnd('insert');
    console.time('flush');
    await db.flush();
    console.timeEnd('flush');
    console.time('query');
    const res = db.find({ subject: 'S1', predicate: 'R1' }).all();
    console.timeEnd('query');
    console.log('hits', res.length);
}
// eslint-disable-next-line @typescript-eslint/no-floating-promises
main();
//# sourceMappingURL=bench.js.map