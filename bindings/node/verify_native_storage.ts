
import { PersistentStore } from './src/core/storage/persistentStore.js';
import { rmSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const TEST_DIR = join(process.cwd(), 'temp_verification_db');

// Clean up previous run
try {
    rmSync(TEST_DIR, { recursive: true, force: true });
} catch { }
mkdirSync(TEST_DIR, { recursive: true });

async function runVerification() {
    console.log('Starting Native Storage Verification...');

    const dbPath = join(TEST_DIR, 'data.db');
    const store = await PersistentStore.open(dbPath);

    // Ensure native handle is loaded
    // @ts-ignore
    if (!store.nativeHandle) {
        console.error('❌ Native handle not loaded! Verification failed.');
        process.exit(1);
    }
    console.log('✅ Native handle loaded.');

    // 1. Test Add Fact (which triggers Interning)
    console.log('\nTesting Add Fact & Interning...');
    const s = 'Alice';
    const p = 'knows';
    const o = 'Bob';

    const fact = store.addFact({ subject: s, predicate: p, object: o });
    console.log('✅ Added fact:', fact);

    const sId = fact.subjectId;
    const pId = fact.predicateId;
    const oId = fact.objectId;

    // 2. Verify Interning
    const sVal = store.getNodeValueById(sId);
    if (sVal !== s) {
        console.error(`❌ Failed to resolve ID ${sId} back to ${s}. Got ${sVal}`);
        process.exit(1);
    }
    console.log(`✅ Resolved ID ${sId} back to string "${sVal}".`);

    const lookupId = store.getNodeIdByValue(s);
    if (lookupId !== sId) {
        console.error(`❌ Failed to lookup ID for "${s}". Expected ${sId}, got ${lookupId}`);
        process.exit(1);
    }
    console.log(`✅ Looked up string "${s}" to ID ${lookupId}.`);

    // 3. Test Query
    console.log('\nTesting Query...');
    // PersistentStore.query takes IDs
    const results = store.query({ subjectId: sId, predicateId: pId, objectId: oId });
    if (results.length !== 1) {
        console.error(`❌ Query failed. Expected 1 result, got ${results.length}`);
        process.exit(1);
    }
    console.log('✅ Query returned correct result.');

    // 4. Test Properties
    console.log('\nTesting Properties...');
    const props = { age: 30, active: true };
    store.setNodeProperties(sId, props);
    console.log('✅ setNodeProperties executed without error.');

    // 5. Test Delete Fact
    console.log('\nTesting Delete Fact...');
    store.deleteFact({ subject: s, predicate: p, object: o });

    const resultsAfterDelete = store.query({ subjectId: sId, predicateId: pId, objectId: oId });
    if (resultsAfterDelete.length !== 0) {
        console.error(`❌ Delete failed. Expected 0 results, got ${resultsAfterDelete.length}`);
        process.exit(1);
    }
    console.log('✅ Delete successful.');

    store.close();
    console.log('\n🎉 All Native Storage verifications passed!');
}

runVerification().catch(e => {
    console.error(e);
    process.exit(1);
});
