import { existsSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const requireNative = createRequire(import.meta.url);
const bindingDirUrl = new URL('../../native/nervusdb-node/npm/', import.meta.url);
const bindingDirPath = fileURLToPath(bindingDirUrl);

function resolveCandidateFilenames(): string[] {
  const { platform, arch } = process;
  const candidates: string[] = [];

  switch (platform) {
    case 'darwin': {
      candidates.push(`index.darwin-${arch}.node`, 'index.darwin-universal.node');
      break;
    }
    case 'linux': {
      if (arch === 'x64') {
        candidates.push('index.linux-x64-gnu.node', 'index.linux-x64-musl.node');
      } else if (arch === 'arm64') {
        candidates.push('index.linux-arm64-gnu.node', 'index.linux-arm64-musl.node');
      }
      break;
    }
    case 'win32': {
      if (arch === 'x64') {
        candidates.push('index.win32-x64-msvc.node');
      } else if (arch === 'arm64') {
        candidates.push('index.win32-arm64-msvc.node');
      } else if (arch === 'ia32') {
        candidates.push('index.win32-ia32-msvc.node');
      }
      break;
    }
    case 'android': {
      if (arch === 'arm64') {
        candidates.push('index.android-arm64.node');
      } else if (arch === 'arm') {
        candidates.push('index.android-arm-eabi.node');
      }
      break;
    }
    case 'freebsd': {
      if (arch === 'x64') {
        candidates.push('index.freebsd-x64.node');
      }
      break;
    }
  }

  candidates.push('index.node');
  return candidates;
}

function resolveBindingPath(): string {
  const candidates = resolveCandidateFilenames().map((name) =>
    fileURLToPath(new URL(name, bindingDirUrl)),
  );

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `Native binding not found. Checked: ${candidates
      .map((candidate) => candidate.replace(bindingDirPath, ''))
      .join(', ')}`,
  );
}

function ensureNativeBinary(): void {
  try {
    resolveBindingPath();
    return;
  } catch (err) {
    // Missing binary, fall through to build
    if (process.env.NERVUSDB_SKIP_NATIVE_BUILD === '1') {
      throw err;
    }
  }

  execSync(
    'pnpm exec napi build --release --platform --cargo-cwd native/nervusdb-node native/nervusdb-node/npm',
    {
      stdio: 'inherit',
      cwd: process.cwd(),
    },
  );
}

export function loadNativeBinding<T extends Record<string, unknown>>(): T {
  ensureNativeBinary();
  const bindingPath = resolveBindingPath();
  return requireNative(bindingPath) as T;
}
