import { openNativeHandle } from './src/native/core.js';
import { join } from 'path';
import { tmpdir } from 'os';
import { mkdtempSync, rmSync } from 'fs';

const tmpDir = mkdtempSync(join(tmpdir(), 'nervusdb-verify-props-'));
const dbPath = join(tmpDir, 'test.db');

console.log(`Using temporary database at: ${dbPath}`);

try {
    const db = openNativeHandle(dbPath);
    if (!db) {
        console.error('Failed to open native handle');
        process.exit(1);
    }

    console.log('Database opened successfully');

    // Test Node Properties
    const nodeId = 1;
    const nodeProps = { name: 'Alice', age: 30 };
    console.log(`Setting node property for ID ${nodeId}:`, nodeProps);
    db.setNodeProperty(nodeId, JSON.stringify(nodeProps));

    const retrievedNodePropsJson = db.getNodeProperty(nodeId);
    console.log(`Retrieved node property for ID ${nodeId}:`, retrievedNodePropsJson);

    if (retrievedNodePropsJson) {
        const retrievedNodeProps = JSON.parse(retrievedNodePropsJson);
        if (retrievedNodeProps.name === nodeProps.name && retrievedNodeProps.age === nodeProps.age) {
            console.log('✅ Node property verification passed');
        } else {
            console.error('❌ Node property verification failed: content mismatch');
        }
    } else {
        console.error('❌ Node property verification failed: not found');
    }

    // Test Edge Properties
    const s = 1, p = 2, o = 3;
    const edgeProps = { weight: 0.5, since: '2023-01-01' };
    console.log(`Setting edge property for ${s}-${p}-${o}:`, edgeProps);
    db.setEdgeProperty(s, p, o, JSON.stringify(edgeProps));

    const retrievedEdgePropsJson = db.getEdgeProperty(s, p, o);
    console.log(`Retrieved edge property for ${s}-${p}-${o}:`, retrievedEdgePropsJson);

    if (retrievedEdgePropsJson) {
        const retrievedEdgeProps = JSON.parse(retrievedEdgePropsJson);
        if (retrievedEdgeProps.weight === edgeProps.weight && retrievedEdgeProps.since === edgeProps.since) {
            console.log('✅ Edge property verification passed');
        } else {
            console.error('❌ Edge property verification failed: content mismatch');
        }
    } else {
        console.error('❌ Edge property verification failed: not found');
    }

    db.close();

} catch (err) {
    console.error('An error occurred:', err);
} finally {
    try {
        rmSync(tmpDir, { recursive: true, force: true });
        console.log('Cleaned up temporary directory');
    } catch (e) {
        console.error('Failed to cleanup:', e);
    }
}
