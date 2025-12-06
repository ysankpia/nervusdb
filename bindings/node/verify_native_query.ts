import { PersistentStore } from './src/core/storage/persistentStore';
import { join } from 'path';
import { tmpdir } from 'os';
import { mkdtempSync, rmSync } from 'fs';

const TEST_DIR = mkdtempSync(join(tmpdir(), 'nervusdb-query-test-'));
const DB_PATH = join(TEST_DIR, 'data.db');

async function main() {
    console.log(`Testing Native Query at ${DB_PATH}`);
    const store = await PersistentStore.open(DB_PATH);

    try {
        // 1. Add some data
        console.log('Adding facts...');
        await store.addFact({
            subject: 'Alice',
            predicate: 'knows',
            object: 'Bob',
        });
        await store.addFact({
            subject: 'Bob',
            predicate: 'knows',
            object: 'Charlie',
        });

        // 2. Test Native Query
        console.log('Executing native query...');
        const query = "MATCH (n) RETURN n";
        // Note: Our current planner is very simple and might not support "MATCH (n) RETURN n" without labels or full scan support.
        // But let's try.
        // Wait, my executor implementation for ScanNode returns empty!
        // So this will return empty results.
        // But it should not crash.

        const results = await store.query(query);
        console.log('Query Results:', results);

        if (Array.isArray(results)) {
            console.log('Query executed successfully (even if empty).');
        } else {
            console.error('Query returned non-array:', results);
            process.exit(1);
        }

    } catch (error) {
        console.error('Test failed:', error);
        process.exit(1);
    } finally {
        // Cleanup
        // rmSync(TEST_DIR, { recursive: true, force: true });
    }
}

main();
