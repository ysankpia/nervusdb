// cspell configuration for SynapseDB
/** @type {import('cspell').CSpellUserSettings} */
module.exports = {
  version: '0.2',
  language: 'en',
  allowCompoundWords: true,
  ignorePaths: [
    'bindings/node/node_modules',
    'bindings/node/dist',
    'bindings/node/coverage',
    '**/*.synapsedb',
    '**/*.synapsedb.*',
    '**/*.idxpage',
    '.git',
    '.husky/_',
    'bindings/node/pnpm-lock.yaml',
    'bindings/node/native/nervusdb-node/npm'
  ],
  words: [
    'SynapseDB',
    'SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS',
    'WAL', 'fsync', 'brotli', 'crc32', 'idxpage', 'manifest', 'orphans', 'txId', 'txids',
    'Vitest', 'tsconfig', 'tsx', 'pnpm', 'eslint', 'precommit', 'prepush',
    'LSM', 'hotness', 'readers'
  ]
};
