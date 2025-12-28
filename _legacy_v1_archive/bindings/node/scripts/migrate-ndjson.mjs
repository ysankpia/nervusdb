#!/usr/bin/env node
/**
 * 迁移脚本：从 NDJSON(每行一个 {subject,predicate,object,props?}) 导入到 NervusDB
 * 用法：node scripts/migrate-ndjson.mjs <input.ndjson> <db.synapsedb>
 */
import { createReadStream } from 'node:fs';
import { createInterface } from 'node:readline';
import { NervusDB } from '../dist/nervusDb.js';

async function main() {
  const [input, dbPath] = process.argv.slice(2);
  if (!input || !dbPath) {
    console.error('用法: node scripts/migrate-ndjson.mjs <input.ndjson> <db.synapsedb>');
    process.exit(1);
  }
  const db = await NervusDB.open(dbPath);
  const rl = createInterface({ input: createReadStream(input, 'utf8'), crlfDelay: Infinity });
  let n = 0;
  for await (const line of rl) {
    const s = line.trim();
    if (s.length === 0) continue;
    const obj = JSON.parse(s);
    const { subject, predicate, object, subjectProperties, objectProperties, edgeProperties } = obj;
    if (!subject || !predicate || !object) continue;
    db.addFact(
      { subject, predicate, object },
      { subjectProperties, objectProperties, edgeProperties },
    );
    n += 1;
    if (n % 10000 === 0) await db.flush();
  }
  await db.flush();
  await db.close();
  console.log(`导入完成: ${n} 条`);
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

