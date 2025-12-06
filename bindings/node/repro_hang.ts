
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { rmSync, mkdirSync } from 'node:fs';
import { PersistentStore } from './src/core/storage/persistentStore.js';

async function run() {
    const unique = `repro-hang-${Date.now()}`;
    const testDir = join(tmpdir(), unique);
    mkdirSync(testDir, { recursive: true });
    const dbPath = join(testDir, 'test.db');

    console.log('--- Starting Repro ---');
    console.log('DB Path:', dbPath);

    try {
        console.log('Opening store...');
        const store = await PersistentStore.open(dbPath, { enableLock: true });
        console.log('Store opened.');

        console.log('Adding 5000 facts...');
        for (let i = 0; i < 5000; i++) {
            store.addFact({
                subject: `s${i}`,
                predicate: 'p',
                object: `o${i}`,
            });
        }
        console.log('Facts added.');

        console.log('Flushing...');
        await store.flush();
        console.log('Flushed.');

        console.log('Closing store...');
        await store.close();
        console.log('Store closed.');
    } catch (err) {
        console.error('Error:', err);
    } finally {
        try {
            rmSync(testDir, { recursive: true, force: true });
        } catch { }
    }
    console.log('--- Finished Repro ---');
}

run();
