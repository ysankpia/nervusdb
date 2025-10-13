#!/usr/bin/env node
/**
 * NervusDB Memory Leak Analysis Script
 * 
 * Usage:
 *   node --expose-gc scripts/memory-leak-analysis.mjs
 * 
 * This script:
 * 1. Runs multiple iterations of DB operations
 * 2. Monitors memory growth
 * 3. Generates heap snapshots for analysis
 * 4. Identifies potential memory leaks
 */

import { NervusDB } from '../dist/index.mjs';
import { writeHeapSnapshot } from 'v8';
import fs from 'fs/promises';
import path from 'path';
import os from 'os';

const ITERATIONS = 10;
const RECORDS_PER_ITERATION = 1000;
const SNAPSHOT_DIR = './memory-snapshots';

async function ensureSnapshotDir() {
  try {
    await fs.mkdir(SNAPSHOT_DIR, { recursive: true });
  } catch (err) {
    // Ignore if exists
  }
}

function formatBytes(bytes) {
  return Math.round(bytes / 1024 / 1024);
}

function getMemoryStats() {
  const mem = process.memoryUsage();
  return {
    heapUsed: formatBytes(mem.heapUsed),
    heapTotal: formatBytes(mem.heapTotal),
    external: formatBytes(mem.external),
    rss: formatBytes(mem.rss),
  };
}

async function runIteration(i) {
  const dbPath = path.join(os.tmpdir(), `nervusdb-leak-test-${i}.db`);

  console.log(`\n🔄 Iteration ${i + 1}/${ITERATIONS}`);
  console.log(`   DB Path: ${dbPath}`);

  const db = await NervusDB.open(dbPath);

  // Insert data
  console.log(`   ➕ Inserting ${RECORDS_PER_ITERATION} records...`);
  const startInsert = Date.now();
  for (let j = 0; j < RECORDS_PER_ITERATION; j++) {
    db.addFact({
      subject: `subject_${i}_${j}`,
      predicate: `predicate_${j % 10}`,
      object: `object_${j}`,
      subjectProperties: {
        iteration: i,
        index: j,
        timestamp: Date.now(),
      },
    });
  }
  await db.flush();
  const insertTime = Date.now() - startInsert;
  console.log(`   ✅ Insert completed in ${insertTime}ms`);

  // Query data
  console.log(`   🔍 Querying data...`);
  const startQuery = Date.now();
  const results = db.find({ subject: `subject_${i}_500` }).all();
  const queryTime = Date.now() - startQuery;
  console.log(`   ✅ Query completed in ${queryTime}ms (found ${results.length} results)`);

  // Close DB
  await db.close();

  // Clean up DB file
  try {
    await fs.rm(dbPath, { recursive: true, force: true });
  } catch (err) {
    console.warn(`   ⚠️  Failed to clean up ${dbPath}: ${err.message}`);
  }

  // Force GC if available
  if (global.gc) {
    global.gc();
    console.log(`   🗑️  Forced garbage collection`);
  } else {
    console.log(`   ⚠️  GC not available (run with --expose-gc)`);
  }

  const mem = getMemoryStats();
  console.log(`   📊 Memory: Heap=${mem.heapUsed}MB, RSS=${mem.rss}MB, External=${mem.external}MB`);

  return {
    iteration: i,
    ...mem,
    insertTime,
    queryTime,
  };
}

async function analyzeMemoryLeak() {
  console.log('🔍 NervusDB Memory Leak Analysis');
  console.log('=' .repeat(60));
  console.log(`Iterations: ${ITERATIONS}`);
  console.log(`Records per iteration: ${RECORDS_PER_ITERATION}`);
  console.log(`Snapshot directory: ${SNAPSHOT_DIR}`);
  console.log('=' .repeat(60));

  await ensureSnapshotDir();

  const snapshots = [];

  // Initial memory
  const initialMem = getMemoryStats();
  console.log(`\n📊 Initial Memory:`);
  console.log(`   Heap: ${initialMem.heapUsed}MB`);
  console.log(`   RSS: ${initialMem.rss}MB`);

  // Run iterations
  for (let i = 0; i < ITERATIONS; i++) {
    const stats = await runIteration(i);
    snapshots.push(stats);

    // Generate heap snapshot at specific iterations
    if (i === 2 || i === 5 || i === ITERATIONS - 1) {
      console.log(`   💾 Generating heap snapshot...`);
      const snapshotPath = path.join(SNAPSHOT_DIR, `heap-snapshot-${i}.heapsnapshot`);
      writeHeapSnapshot(snapshotPath);
      console.log(`   ✅ Snapshot saved: ${snapshotPath}`);
    }

    // Wait a bit between iterations
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  // Final memory
  const finalMem = getMemoryStats();

  // Analysis
  console.log('\n' + '='.repeat(60));
  console.log('📊 Memory Growth Analysis');
  console.log('='.repeat(60));

  console.log('\n📈 Memory Timeline:');
  console.table(
    snapshots.map((s) => ({
      Iteration: s.iteration,
      'Heap (MB)': s.heapUsed,
      'RSS (MB)': s.rss,
      'External (MB)': s.external,
      'Insert (ms)': s.insertTime,
      'Query (ms)': s.queryTime,
    }))
  );

  const heapGrowth = finalMem.heapUsed - initialMem.heapUsed;
  const rssGrowth = finalMem.rss - initialMem.rss;

  console.log('\n📏 Growth Summary:');
  console.log(`   Initial Heap: ${initialMem.heapUsed}MB`);
  console.log(`   Final Heap: ${finalMem.heapUsed}MB`);
  console.log(`   Heap Growth: ${heapGrowth}MB`);
  console.log('');
  console.log(`   Initial RSS: ${initialMem.rss}MB`);
  console.log(`   Final RSS: ${finalMem.rss}MB`);
  console.log(`   RSS Growth: ${rssGrowth}MB`);

  // Verdict
  console.log('\n🔬 Verdict:');
  if (heapGrowth > 50) {
    console.log(`   ⚠️  POTENTIAL MEMORY LEAK DETECTED!`);
    console.log(`   Heap grew by ${heapGrowth}MB over ${ITERATIONS} iterations`);
    console.log(`   Expected: < 50MB, Actual: ${heapGrowth}MB`);
    console.log('');
    console.log('   📝 Next Steps:');
    console.log('   1. Analyze heap snapshots in Chrome DevTools');
    console.log(`   2. Compare snapshots: iteration-2 vs iteration-${ITERATIONS - 1}`);
    console.log('   3. Look for objects that are not being garbage collected');
    console.log('   4. Check for:');
    console.log('      - Unclosed file handles');
    console.log('      - Event listeners not removed');
    console.log('      - Circular references');
    console.log('      - Unbounded caches');
  } else if (heapGrowth > 20) {
    console.log(`   ⚠️  Moderate memory growth detected`);
    console.log(`   Heap grew by ${heapGrowth}MB (acceptable but could be optimized)`);
  } else {
    console.log(`   ✅ Memory usage appears stable`);
    console.log(`   Heap growth: ${heapGrowth}MB (well within acceptable range)`);
  }

  // Performance summary
  const avgInsert = snapshots.reduce((sum, s) => sum + s.insertTime, 0) / ITERATIONS;
  const avgQuery = snapshots.reduce((sum, s) => sum + s.queryTime, 0) / ITERATIONS;

  console.log('\n⚡ Performance Summary:');
  console.log(`   Average Insert Time: ${Math.round(avgInsert)}ms`);
  console.log(`   Average Query Time: ${Math.round(avgQuery)}ms`);

  // Heap snapshot analysis instructions
  console.log('\n📚 How to Analyze Heap Snapshots:');
  console.log('   1. Open Chrome DevTools');
  console.log('   2. Go to Memory tab');
  console.log('   3. Click "Load" and select a .heapsnapshot file');
  console.log('   4. Compare different snapshots to find leaks');
  console.log('   5. Look for "Detached" objects or large retained sizes');

  console.log('\n✨ Analysis complete!');
  console.log('='.repeat(60));
}

// Run analysis
analyzeMemoryLeak().catch((err) => {
  console.error('\n❌ Analysis failed:', err);
  process.exit(1);
});
