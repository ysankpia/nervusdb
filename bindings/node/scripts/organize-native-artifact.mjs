#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs';
import { copyFileSync } from 'node:fs';
import { join } from 'node:path';

const platform = process.argv[2];
if (!platform) {
  console.error('Usage: node scripts/organize-native-artifact.mjs <platform>');
  process.exit(1);
}

const baseDir = join(process.cwd(), 'native', 'nervusdb-node', 'npm');
if (!existsSync(baseDir)) {
  console.error(`Native artifact directory not found: ${baseDir}`);
  process.exit(1);
}

const entries = readdirSync(baseDir);
const nodeFiles = entries.filter((name) => {
  const fullPath = join(baseDir, name);
  try {
    return statSync(fullPath).isFile() && name.endsWith('.node');
  } catch {
    return false;
  }
});

if (nodeFiles.length === 0) {
  console.error(`No .node artifacts found under ${baseDir}`);
  process.exit(1);
}

const platformMatch = nodeFiles.find((name) => name.includes(platform));
let candidate = platformMatch;
if (!candidate) {
  const tokens = platform.split('-');
  if (tokens.length >= 2) {
    const shortToken = `${tokens[0]}-${tokens[1]}`;
    candidate = nodeFiles.find((name) => name.includes(shortToken));
  }
}
if (!candidate) {
  candidate = nodeFiles.find((name) => name !== 'index.node');
}
if (!candidate) {
  candidate = nodeFiles.find((name) => name === 'index.node');
}
if (!candidate) {
  console.error(`Unable to select native artifact for platform ${platform}`);
  process.exit(1);
}

const destinationDir = join(baseDir, platform);
mkdirSync(destinationDir, { recursive: true });

const destinationFile = join(destinationDir, 'index.node');
if (existsSync(destinationFile)) {
  rmSync(destinationFile);
}
copyFileSync(join(baseDir, candidate), destinationFile);

console.log(`Organized native artifact for ${platform}: ${candidate} -> ${destinationFile}`);
