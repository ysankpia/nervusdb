import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function fail(message) {
  process.stderr.write(`[contract] ${message}\n`);
  process.exit(1);
}

function assert(condition, message) {
  if (!condition) fail(message);
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../..');

const headerPath = path.join(repoRoot, 'nervusdb-core/include/nervusdb.h');
const nodeTsPath = path.join(repoRoot, 'bindings/node/src/nervusDb.ts');
const napiDtsPath = path.join(repoRoot, 'bindings/node/native/nervusdb-node/npm/index.d.ts');

for (const p of [headerPath, nodeTsPath, napiDtsPath]) {
  assert(fs.existsSync(p), `missing file: ${p}`);
}

const header = fs.readFileSync(headerPath, 'utf8');
const nodeTs = fs.readFileSync(nodeTsPath, 'utf8');
const napiDts = fs.readFileSync(napiDtsPath, 'utf8');

function parseCValueTypes(text) {
  const map = new Map();
  const re = /NERVUSDB_VALUE_([A-Z0-9_]+)\s*=\s*(\d+)/g;
  for (const m of text.matchAll(re)) {
    map.set(m[1], Number(m[2]));
  }
  return map;
}

function parseTsEnum(text, enumName) {
  const re = new RegExp(`export\\s+enum\\s+${enumName}\\s*{([\\s\\S]*?)}`, 'm');
  const m = text.match(re);
  assert(m, `missing TS enum: ${enumName}`);
  const body = m[1];
  const map = new Map();
  for (const line of body.split('\n')) {
    const mm = line.match(/^\s*([A-Za-z0-9_]+)\s*=\s*(\d+)\s*,?\s*$/);
    if (!mm) continue;
    map.set(mm[1], Number(mm[2]));
  }
  return map;
}

function parseDtsClassMethods(text, className) {
  const re = new RegExp(`export\\s+declare\\s+class\\s+${className}\\s*{([\\s\\S]*?)^}`, 'm');
  const m = text.match(re);
  assert(m, `missing d.ts class: ${className}`);
  const body = m[1];
  const methods = new Set();
  for (const mm of body.matchAll(/^\s*([A-Za-z0-9_]+)\s*\(/gm)) {
    methods.add(mm[1]);
  }
  return methods;
}

const cValueTypes = parseCValueTypes(header);
const tsValueTypes = parseTsEnum(nodeTs, 'CypherValueType');

const expected = new Map([
  ['NULL', 0],
  ['TEXT', 1],
  ['FLOAT', 2],
  ['BOOL', 3],
  ['NODE', 4],
  ['RELATIONSHIP', 5],
]);

for (const [key, val] of expected) {
  assert(cValueTypes.get(key) === val, `C value type mismatch: ${key} expected ${val}`);
}

const tsExpected = new Map([
  ['Null', 0],
  ['Text', 1],
  ['Float', 2],
  ['Bool', 3],
  ['Node', 4],
  ['Relationship', 5],
]);
for (const [key, val] of tsExpected) {
  assert(tsValueTypes.get(key) === val, `TS CypherValueType mismatch: ${key} expected ${val}`);
}

const stmtMethods = parseDtsClassMethods(napiDts, 'StatementHandle');
const dbMethods = parseDtsClassMethods(napiDts, 'DatabaseHandle');

for (const m of [
  'step',
  'columnCount',
  'columnName',
  'columnType',
  'columnText',
  'columnFloat',
  'columnBool',
  'columnNodeId',
  'columnRelationship',
  'finalize',
]) {
  assert(stmtMethods.has(m), `Node StatementHandle missing method: ${m}`);
}

assert(dbMethods.has('prepareV2'), 'Node DatabaseHandle missing method: prepareV2');

process.stdout.write('[contract] ok\n');

