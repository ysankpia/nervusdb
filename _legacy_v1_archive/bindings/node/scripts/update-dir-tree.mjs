/*
 * 自动生成并更新 `.agents/rules/base.md` 中的“目录结构（详细）”代码块
 * 使用：pnpm run docs:tree （由 pre-commit 钩子自动调用）
 */
import fs from 'node:fs/promises';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const PROJECT_ROOT = path.resolve(process.cwd(), '..', '..');
const TARGET_FILE = path.join(PROJECT_ROOT, '.agents/rules/base.md');
const HEADER = '目录结构（详细）：';

const COMMENTS = {
  'src': '源码目录',
  'src/index.ts': '顶层导出与连接工具',
  'src/synapseDb.ts': '数据库主 API',
  'src/typedSynapseDb.ts': '类型安全包装器',
  'src/query': '查询构建器与链式联想',
  'src/query/queryBuilder.ts': '链式查询构建器',
  'src/query/aggregation.ts': '聚合查询',
  'src/query/iterator.ts': '异步批量迭代器',
  'src/query/path': '路径/变长路径',
  'src/query/path/variable.ts': '变长路径构建器',
  'src/query/pattern': '模式匹配',
  'src/query/pattern/match.ts': '模式匹配执行',
  'src/storage': '持久化/索引/字典/WAL 等',
  'src/storage/dictionary.ts': '字典存储',
  'src/storage/tripleStore.ts': '三元组存储',
  'src/storage/tripleIndexes.ts': '六序索引入口/选择',
  'src/storage/pagedIndex.ts': '分页化磁盘索引',
  'src/storage/propertyStore.ts': '属性值存储',
  'src/storage/propertyIndex.ts': '属性索引',
  'src/storage/persistentStore.ts': '持久化抽象',
  'src/storage/staging.ts': '暂存层/增量段',
  'src/storage/wal.ts': '写前日志（WAL v2）',
  'src/storage/readerRegistry.ts': '读者登记/一致性',
  'src/storage/txidRegistry.ts': '事务批次与幂等',
  'src/storage/hotness.ts': '热度统计与半衰',
  'src/storage/layout.ts': '文件布局与常量',
  'src/storage/fileHeader.ts': '主文件头部结构',
  'src/maintenance': '维护与治理工具逻辑',
  'src/maintenance/check.ts': '校验与诊断',
  'src/maintenance/repair.ts': '修复器',
  'src/maintenance/compaction.ts': '整序/增量压实',
  'src/maintenance/autoCompact.ts': '自动压实策略',
  'src/maintenance/gc.ts': '页面级 GC',
  'src/cli': '命令行入口（开发期）',
  'src/cli/synapsedb.ts': '顶层 CLI（发布后由 dist/bin 提供）',
  'src/cli/check.ts': 'db:check 子命令',
  'src/cli/compact.ts': 'db:compact 子命令',
  'src/cli/dump.ts': 'db:dump 子命令',
  'src/cli/stats.ts': 'db:stats 子命令',
  'src/cli/txids.ts': 'db:txids 子命令',
  'src/cli/gc.ts': 'db:gc 子命令',
  'src/cli/readers.ts': 'db:readers 子命令',
  'src/cli/hot.ts': 'db:hot 子命令',
  'src/cli/bench.ts': 'bench 子命令',
  'src/cli/repair_page.ts': 'db:repair-page 子命令',
  'src/cli/auto_compact.ts': 'db:auto-compact 子命令',
  'src/utils': '通用工具',
  'src/utils/fault.ts': '自定义错误与故障注入',
  'src/utils/lock.ts': '文件锁/进程级互斥',
  'src/types': '公共类型声明',
  'src/types/openOptions.ts': 'open() 选项类型',
  'src/types/enhanced.ts': '类型系统增强',
  'src/graph': '图功能/标签与路径',
  'src/graph/labels.ts': '标签系统',
  'src/graph/paths.ts': '图路径工具',
  'src/test': '辅助测试资源（若有）',
  'tests': '单元与集成测试（Vitest）',
  'docs': '文档与示例',
  'docs/NervusDB设计文档.md': '设计说明',
  'docs/使用示例': '使用教程与FAQ',
  'docs/教学文档': '系列教程与API',
  'docs/milestones': '里程碑规划',
  'docs/项目发展路线图': 'Roadmap',
  'docs/项目实施建议': '推广与落地建议',
  'docs/项目审查文档': '多模型评审记录',
  'benchmarks': '性能基准脚本',
  'scripts': '辅助脚本',
  '.qoder/repowiki': '仓内知识库',
  '.qoder/repowiki/zh': '中文资料',
  '.qoder/repowiki/zh/content': '中文内容',
  '.qoder/repowiki/zh/meta': '知识库元数据',
  '.qoder/repowiki/zh/meta/repowiki-metadata.json': '知识库索引信息',
  'dist': '构建产物（发布用）',
  'coverage': '覆盖率报告（测试生成）',
  '.agents': '智能体规则与协作说明',
  '.agents/rules/base.md': '本文件（AGENTS/CLAUDE 软链接）',
  '.github': 'CI/Issue 模板等',
  '.husky': 'Git hooks（pre-commit/pre-push）',
  'package.json': '脚本与依赖',
  'tsconfig.json': 'TypeScript 编译配置',
  'tsconfig.vitest.json': '测试 TypeScript 配置',
  'vitest.config.ts': '测试配置',
  'eslint.config.js': 'Lint 配置',
  '.prettierrc': 'Prettier 配置',
  '.prettierignore': 'Prettier 忽略',
  '.lintstaged.cjs': '提交前 Lint 配置',
  '.gitignore': 'Git 忽略',
  'README.md': '仓库概览',
  'CHANGELOG.md': '变更记录',
  'pnpm-lock.yaml': '依赖锁',
};

const PATH_ALIAS_RULES = [
  { virtual: 'src', actual: 'bindings/node/src' },
  { virtual: 'tests', actual: 'bindings/node/tests' },
  { virtual: 'benchmarks', actual: 'bindings/node/benchmarks' },
  { virtual: 'scripts', actual: 'bindings/node/scripts' },
  { virtual: 'dist', actual: 'bindings/node/dist' },
  { virtual: 'coverage', actual: 'bindings/node/coverage' },
  { virtual: 'package.json', actual: 'bindings/node/package.json' },
  { virtual: 'tsconfig.json', actual: 'bindings/node/tsconfig.json' },
  { virtual: 'tsconfig.build.json', actual: 'bindings/node/tsconfig.build.json' },
  { virtual: 'tsconfig.vitest.json', actual: 'bindings/node/tsconfig.vitest.json' },
  { virtual: 'vitest.config.ts', actual: 'bindings/node/vitest.config.ts' },
  { virtual: 'eslint.config.js', actual: 'bindings/node/eslint.config.js' },
  { virtual: 'build.config.mjs', actual: 'bindings/node/build.config.mjs' },
  { virtual: 'build.advanced.mjs', actual: 'bindings/node/build.advanced.mjs' },
  { virtual: 'pnpm-lock.yaml', actual: 'bindings/node/pnpm-lock.yaml' },
  { virtual: 'temp_verification_db', actual: 'bindings/node/temp_verification_db' },
  { virtual: 'temp_verification_db.redb', actual: 'bindings/node/temp_verification_db.redb' },
  { virtual: '.husky', actual: '.husky' }
];

const withComment = (rel) => {
  const c = COMMENTS[rel];
  return c ? `${rel}  ${c}` : rel;
};

// --- 使用 git 列表以遵循 .gitignore 规则 ---
function toPosix(p) {
  return p.replace(/\\/g, '/').replace(/^\.\//, '');
}

function resolveVirtualPath(rel) {
  const normalized = toPosix(rel);
  for (const { virtual, actual } of PATH_ALIAS_RULES) {
    if (normalized === virtual) return actual;
    if (normalized.startsWith(`${virtual}/`)) {
      return `${actual}${normalized.slice(virtual.length)}`;
    }
  }
  return normalized;
}

function toVirtualPath(actual) {
  const normalized = toPosix(actual);
  for (const { virtual, actual: real } of PATH_ALIAS_RULES) {
    if (normalized === real) return virtual;
    if (normalized.startsWith(`${real}/`)) {
      return `${virtual}${normalized.slice(real.length)}`;
    }
  }
  return normalized;
}

function readGitFiles() {
  try {
    const tracked = execFileSync('git', ['ls-files', '-z'], { cwd: PROJECT_ROOT });
    const others = execFileSync('git', ['ls-files', '--others', '--exclude-standard', '-z'], { cwd: PROJECT_ROOT });
    const list = (tracked.toString('utf8') + others.toString('utf8'))
      .split('\u0000')
      .map((s) => s.trim())
      .filter(Boolean)
      .map(toPosix);
    const set = new Set(list);
    return { list, set };
  } catch (e) {
    // 非 git 环境时降级为空集合（避免误收未跟踪且被忽略的目录）
    return { list: [], set: new Set() };
  }
}

const GIT = readGitFiles();

function hasFile(rel) {
  const actual = resolveVirtualPath(rel);
  return GIT.set.has(actual);
}

function hasDir(rel) {
  const prefix = resolveVirtualPath(rel).replace(/\/?$/, '/');
  return GIT.list.some((f) => f.startsWith(prefix));
}

function listDirFiles(rel, { ext, directOnly = true } = { ext: '', directOnly: true }) {
  const prefix = resolveVirtualPath(rel).replace(/\/?$/, '/');
  const out = [];
  for (const f of GIT.list) {
    if (!f.startsWith(prefix)) continue;
    if (ext && !f.endsWith(ext)) continue;
    if (directOnly) {
      const rest = f.slice(prefix.length);
      if (rest.includes('/')) continue;
    }
    out.push(toVirtualPath(f));
  }
  return out.sort();
}

function listDirEntries(rel) {
  const prefix = resolveVirtualPath(rel).replace(/\/?$/, '/');
  const entries = new Map();
  for (const f of GIT.list) {
    if (!f.startsWith(prefix)) continue;
    const rest = f.slice(prefix.length);
    if (!rest) continue;
    const [head, ...tail] = rest.split('/');
    if (!head) continue;
    const kind = tail.length > 0 ? 'dir' : 'file';
    if (kind === 'dir') {
      entries.set(head, 'dir');
    } else if (!entries.has(head)) {
      entries.set(head, 'file');
    }
  }
  return Array.from(entries.entries())
    .map(([name, kind]) => ({ name, kind }))
    .sort((a, b) => a.name.localeCompare(b.name, 'zh-Hans-u-co-pinyin'));
}

async function buildTestsSummary() {
  if (!hasDir('tests')) return [];
  const entries = listDirFiles('tests', { directOnly: true }).map((p) => p.split('/').pop());
  const has = (prefix) => entries.some((f) => f.startsWith(prefix));
  const hasLike = (re) => entries.some((f) => re.test(f));
  const lines = [];
  if (has('wal_') || hasLike(/^wal.*\.test\.ts$/)) lines.push('│  ├─ wal_*.test.ts              WAL 行为/幂等/截断/事务');
  if (has('compaction') || has('compaction_')) lines.push('│  ├─ compaction*.test.ts        压实相关测试');
  if (has('property_index') || has('property_index_')) lines.push('│  ├─ property_index*.test.ts    属性索引功能/性能');
  if (has('query') || has('queryBuilder') || has('query_')) lines.push('│  ├─ query*.test.ts             查询与链式联想');
  if (has('snapshot_')) lines.push('│  ├─ snapshot_*.test.ts         快照一致性与内存占用');
  if (has('performance_')) lines.push('│  ├─ performance_*.test.ts      基线/大数据性能');
  lines.push('│  └─ ...                        其余主题参见文件名');
  return lines;
}

async function buildCliFiles() {
  if (!hasDir('src/cli')) return [];
  const files = listDirFiles('src/cli', { ext: '.ts', directOnly: true }).map((p) => p.split('/').pop());
  return files.map((f, i) => {
    const rel = path.posix.join('src/cli', f);
    const last = i === files.length - 1;
    const tail = COMMENTS[rel] ? `${rel}  ${COMMENTS[rel]}` : rel;
    const bar = last ? '└' : '├';
    return `│  │  ${bar}─ ${tail}`;
  });
}

async function buildStorageFiles() {
  if (!hasDir('src/storage')) return [];
  const files = listDirFiles('src/storage', { ext: '.ts', directOnly: true }).map((p) => p.split('/').pop());
  return files.map((f, i) => {
    const rel = path.posix.join('src/storage', f);
    const last = i === files.length - 1;
    const tail = COMMENTS[rel] ? `${rel}  ${COMMENTS[rel]}` : rel;
    const bar = last ? '└' : '├';
    return `│  │  ${bar}─ ${tail}`;
  });
}

async function buildSrcSection() {
  const lines = [];
  const hasSrc = hasDir('src');
  if (!hasSrc) return lines;
  lines.push('├─ src/                          ' + (COMMENTS['src'] || ''));

  if (hasFile('src/index.ts'))
    lines.push('│  ├─ ' + withComment('src/index.ts'));
  if (hasFile('src/synapseDb.ts'))
    lines.push('│  ├─ ' + withComment('src/synapseDb.ts'));
  if (hasFile('src/typedSynapseDb.ts'))
    lines.push('│  ├─ ' + withComment('src/typedSynapseDb.ts'));

  if (hasDir('src/query')){
    lines.push('│  ├─ ' + withComment('src/query') + '/');
    const qFiles = ['queryBuilder.ts','aggregation.ts','iterator.ts'];
    const presentQ = qFiles.filter((f)=> hasFile(path.posix.join('src/query', f)));
    presentQ.forEach((f, idx) => {
      const rel = path.posix.join('src/query', f);
      const last = idx === presentQ.length - 1 && !hasDir('src/query/path') && !hasDir('src/query/pattern');
      const bar = last ? '└' : '├';
      lines.push(`│  │  ${bar}─ ${withComment(rel)}`);
    });
    // 子目录：path
    if (hasDir('src/query/path')){
      lines.push('│  │  ├─ ' + withComment('src/query/path') + '/');
      const pFiles = listDirFiles('src/query/path', { ext: '.ts', directOnly: true }).map((p) => p.split('/').pop());
      pFiles.forEach((f, idx) => {
        const rel = path.posix.join('src/query/path', f);
        const last = idx === pFiles.length - 1 && !hasDir('src/query/pattern');
        const bar = last ? '└' : '├';
        lines.push(`│  │  │  ${bar}─ ${withComment(rel)}`);
      });
    }
    // 子目录：pattern
    if (hasDir('src/query/pattern')){
      lines.push('│  │  └─ ' + withComment('src/query/pattern') + '/');
      const mFiles = listDirFiles('src/query/pattern', { ext: '.ts', directOnly: true }).map((p) => p.split('/').pop());
      mFiles.forEach((f, idx) => {
        const rel = path.posix.join('src/query/pattern', f);
        const last = idx === mFiles.length - 1;
        const bar = last ? '└' : '├';
        lines.push(`│  │     ${bar}─ ${withComment(rel)}`);
      });
    }
  }

  if (hasDir('src/storage')){
    lines.push('│  ├─ ' + withComment('src/storage') + '/');
    const storageFiles = await buildStorageFiles();
    lines.push(...storageFiles);
  }

  if (hasDir('src/maintenance')){
    lines.push('│  ├─ ' + withComment('src/maintenance') + '/');
    const files = ['check.ts','repair.ts','compaction.ts','autoCompact.ts','gc.ts'];
    const present = files.filter((f)=> hasFile(path.posix.join('src/maintenance', f)));
    present.forEach((f, idx) => {
      const rel = path.posix.join('src/maintenance', f);
      const last = idx === present.length - 1;
      const bar = last ? '└' : '├';
      lines.push(`│  │  ${bar}─ ${withComment(rel)}`);
    });
  }

  if (hasDir('src/cli')){
    lines.push('│  ├─ ' + withComment('src/cli') + '/');
    const cliFiles = await buildCliFiles();
    lines.push(...cliFiles);
  }

  if (hasDir('src/utils')){
    lines.push('│  ├─ ' + withComment('src/utils') + '/');
    const utils = ['fault.ts','lock.ts'];
    const present = utils.filter((f)=> hasFile(path.posix.join('src/utils', f)));
    present.forEach((f, idx) => {
      const rel = path.posix.join('src/utils', f);
      const last = idx === present.length - 1;
      const bar = last ? '└' : '├';
      lines.push(`│  │  ${bar}─ ${withComment(rel)}`);
    });
  }

  if (hasDir('src/types')){
    lines.push('│  ├─ ' + withComment('src/types') + '/');
    const tFiles = ['openOptions.ts','enhanced.ts'];
    const presentT = tFiles.filter((f)=> hasFile(path.posix.join('src/types', f)));
    presentT.forEach((f, idx) => {
      const rel = path.posix.join('src/types', f);
      const last = idx === presentT.length - 1 && !hasDir('src/graph');
      const bar = last ? '└' : '├';
      lines.push(`│  │  ${bar}─ ${withComment(rel)}`);
    });
  }

  if (hasDir('src/graph')){
    lines.push('│  └─ ' + withComment('src/graph') + '/');
    const gFiles = listDirFiles('src/graph', { ext: '.ts', directOnly: true }).map((p) => p.split('/').pop());
    gFiles.forEach((f, idx) => {
      const rel = path.posix.join('src/graph', f);
      const last = idx === gFiles.length - 1;
      const bar = last ? '└' : '├';
      lines.push(`│     ${bar}─ ${withComment(rel)}`);
    });
  }

  if (hasDir('src/test')){
    lines.push('│  └─ ' + withComment('src/test') + '/');
  }

  return lines;
}

async function buildDocsSection() {
  const lines = [];
  if (!hasDir('docs')) return lines;
  lines.push('├─ docs/                         ' + (COMMENTS['docs'] || ''));
  if (hasFile('docs/NervusDB设计文档.md'))
    lines.push('│  ├─ ' + withComment('docs/NervusDB设计文档.md'));
  const subdirs = ['milestones','使用示例','教学文档','项目发展路线图','项目实施建议','项目审查文档'];
  const present = subdirs.filter((d)=> hasDir(path.posix.join('docs', d)));
  present.forEach((d, idx) => {
    const last = idx === present.length - 1;
    const bar = last ? '└' : '├';
    const rel = path.posix.join('docs', d);
    lines.push(`│  ${bar}─ ${withComment(rel)}/`);
  });
  return lines;
}

async function buildQoderSection() {
  if (!hasDir('.qoder/repowiki')) return [];
  const lines = [];
  lines.push('├─ .qoder/repowiki/              ' + (COMMENTS['.qoder/repowiki'] || ''));

  if (!hasDir('.qoder/repowiki/zh')) {
    return lines;
  }

  const zhEntries = listDirEntries('.qoder/repowiki/zh');
  const zhHasChildren = zhEntries.length > 0;
  const zhLabel = COMMENTS['.qoder/repowiki/zh'] || '.qoder/repowiki/zh';
  lines.push(`│  ${zhHasChildren ? '├' : '└'}─ ${zhLabel}/`);

  zhEntries.forEach((entry, idx) => {
    const isLast = idx === zhEntries.length - 1;
    const connector = isLast ? '└' : '├';
    const entryRel = path.posix.join('.qoder/repowiki/zh', entry.name);
    const label = COMMENTS[entryRel] || entry.name;
    const suffix = entry.kind === 'dir' ? '/' : '';
    lines.push(`│  │  ${connector}─ ${label}${suffix}`);

    if (entry.kind === 'dir' && entry.name === 'content') {
      const contentEntries = listDirEntries(entryRel);
      contentEntries.forEach((child, childIdx) => {
        const childConnector = childIdx === contentEntries.length - 1 ? '└' : '├';
        const childRel = path.posix.join(entryRel, child.name);
        const childLabel = COMMENTS[childRel] || child.name;
        const childSuffix = child.kind === 'dir' ? '/' : '';
        lines.push(`│  │  │  ${childConnector}─ ${childLabel}${childSuffix}`);
      });
    }

    if (entry.kind === 'dir' && entry.name === 'meta') {
      const metaFiles = listDirFiles(entryRel, { directOnly: true });
      metaFiles.forEach((file, fileIdx) => {
        const fileConnector = fileIdx === metaFiles.length - 1 ? '└' : '├';
        const fileLabel = COMMENTS[file] || path.posix.basename(file);
        lines.push(`│  │     ${fileConnector}─ ${fileLabel}`);
      });
    }
  });

  return lines;
}

async function buildAgentsSection() {
  const lines = [];
  if (!hasDir('.agents')) return lines;
  lines.push('├─ .agents/                      ' + (COMMENTS['.agents'] || ''));
  if (hasFile('.agents/rules/base.md'))
    lines.push('│  └─ ' + withComment('.agents/rules/base.md'));
  return lines;
}

async function buildRootFiles() {
  const order = [
    'dist','coverage','.agents','.github','.husky',
    'package.json','tsconfig.json','tsconfig.vitest.json','vitest.config.ts','eslint.config.js',
    '.prettierrc','.prettierignore','.lintstaged.cjs','.gitignore','README.md','CHANGELOG.md','pnpm-lock.yaml'
  ];
  const lines = [];
  if (hasDir('dist'))
    lines.push('├─ ' + withComment('dist') + '/');
  if (hasDir('coverage'))
    lines.push('├─ ' + withComment('coverage') + '/');
  if (hasDir('benchmarks'))
    lines.push('├─ ' + withComment('benchmarks') + '/');
  if (hasDir('scripts'))
    lines.push('├─ ' + withComment('scripts') + '/');
  if (hasDir('.github'))
    lines.push('├─ ' + withComment('.github') + '/');
  if (hasDir('.husky'))
    lines.push('├─ ' + withComment('.husky') + '/');
  const files = order.filter((p) => !['dist','coverage','.agents','.github','.husky'].includes(p));
  const present = files.filter((f) => hasFile(f));
  present.forEach((f, idx) => {
    const last = idx === present.length - 1;
    const bar = last ? '└' : '├';
    lines.push(`${bar}─ ${withComment(f)}`);
  });
  return lines;
}

async function buildTree() {
  const lines = [];
  lines.push('项目根目录');
  lines.push(...(await buildSrcSection()));
  if (hasDir('tests')){
    lines.push('├─ tests/                        ' + (COMMENTS['tests'] || ''));
    lines.push(...(await buildTestsSummary()));
  }
  lines.push(...(await buildDocsSection()));
  lines.push(...(await buildQoderSection()));
  lines.push(...(await buildAgentsSection()));
  lines.push(...(await buildRootFiles()));
  return lines.join('\n');
}

async function updateBaseMd() {
  const content = await fs.readFile(TARGET_FILE, 'utf8');
  const headerIdx = content.indexOf(HEADER);
  if (headerIdx === -1) {
    console.warn(`[update-dir-tree] 未找到段落标题：${HEADER}，跳过更新`);
    return; // 优雅退出，不阻塞 commit
  }
  const fenceStart = content.indexOf('```', headerIdx);
  if (fenceStart === -1) {
    console.error('[update-dir-tree] 未找到目录结构代码块的起始 ```');
    process.exit(2);
  }
  const fenceEnd = content.indexOf('```', fenceStart + 3);
  if (fenceEnd === -1) {
    console.error('[update-dir-tree] 未找到目录结构代码块的结束 ```');
    process.exit(2);
  }
  const tree = await buildTree();
  const newBlock = '```\n' + tree + '\n```';
  const updated = content.slice(0, fenceStart) + newBlock + content.slice(fenceEnd + 3);
  if (updated !== content) {
    await fs.writeFile(TARGET_FILE, updated, 'utf8');
    console.log('[update-dir-tree] base.md 已更新');
  } else {
    console.log('[update-dir-tree] 目录树无变更');
  }
}

updateBaseMd().catch((err) => {
  console.error('[update-dir-tree] 失败:', err);
  process.exit(1);
});
