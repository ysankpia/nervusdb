#!/usr/bin/env node
/**
 * 调试工具：打印某节点的出/入边邻接概览
 * 用法：node scripts/dump-graph.mjs <db.synapsedb> <nodeValue>
 */
import { NervusDB } from '../dist/nervusDb.js';

async function main() {
  const [dbPath, value] = process.argv.slice(2);
  if (!dbPath || !value) {
    console.error('用法: node scripts/dump-graph.mjs <db.synapsedb> <nodeValue>');
    process.exit(1);
  }
  const db = await NervusDB.open(dbPath);
  const out = db.find({ subject: value }).all();
  const inc = db.find({ object: value }).all();
  console.log(`# 出边(${out.length})`);
  for (const e of out) console.log(`${e.subject} -[${e.predicate}]-> ${e.object}`);
  console.log(`# 入边(${inc.length})`);
  for (const e of inc) console.log(`${e.subject} -[${e.predicate}]-> ${e.object}`);
  await db.close();
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

