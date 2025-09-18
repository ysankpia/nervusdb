// cspell configuration for SynapseDB
/** @type {import('cspell').CSpellUserSettings} */
module.exports = {
  version: '0.2',
  language: 'en',
  allowCompoundWords: true,
  ignorePaths: [
    'node_modules',
    'dist',
    'coverage',
    '**/*.synapsedb',
    '**/*.synapsedb.*',
    '**/*.idxpage',
    '.git',
    '.husky/_',
    'pnpm-lock.yaml'
  ],
  words: [
    'SynapseDB',
    'SPO', 'SOP', 'POS', 'PSO', 'OSP', 'OPS',
    'WAL', 'fsync', 'brotli', 'crc32', 'idxpage', 'manifest', 'orphans', 'txId', 'txids',
    'Vitest', 'tsconfig', 'tsx', 'pnpm', 'eslint', 'precommit', 'prepush',
    'LSM', 'hotness', 'readers'
  ]
};
