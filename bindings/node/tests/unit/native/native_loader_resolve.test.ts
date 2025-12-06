import { describe, expect, it, beforeEach, afterEach, vi } from 'vitest';
import { join } from 'node:path';

const fsMock = vi.hoisted(() => ({
  existsSync: vi.fn(),
  readdirSync: vi.fn(),
}));

const moduleMock = vi.hoisted(() => ({
  createRequire: vi.fn(),
}));

vi.mock('node:fs', () => fsMock);
vi.mock('node:module', () => moduleMock);

const makeDirent = (name: string, type: 'file' | 'dir' = 'file') => ({
  name,
  isFile: () => type === 'file',
  isDirectory: () => type === 'dir',
});

describe('native addon path resolution', () => {
  beforeEach(() => {
    vi.resetModules();
    fsMock.existsSync.mockReset();
    fsMock.readdirSync.mockReset();
    moduleMock.createRequire.mockReset();
    delete process.env.NERVUSDB_DISABLE_NATIVE;
    delete process.env.NERVUSDB_EXPECT_NATIVE;
  });

  afterEach(() => {
    delete process.env.NERVUSDB_DISABLE_NATIVE;
    delete process.env.NERVUSDB_EXPECT_NATIVE;
  });

  const setupFileEntries = (fileName: string) => {
    fsMock.existsSync.mockImplementation((path: string) => {
      if (path.endsWith(join('native', 'nervusdb-node', 'index.node'))) {
        return false;
      }
      if (path.endsWith(join('native', 'nervusdb-node', 'npm'))) {
        return true;
      }
      return true;
    });
    fsMock.readdirSync.mockImplementation((path: string) => {
      if (path.endsWith(join('native', 'nervusdb-node', 'npm'))) {
        return [makeDirent(fileName)];
      }
      return [];
    });
  };

  it('loads binding when addon file matches platform/arch', async () => {
    const addonName = `nervusdb-node.${process.platform}.${process.arch}.node`;
    setupFileEntries(addonName);
    const binding = {
      open: vi.fn(() => ({
        addFact: () => ({ subject_id: 1, predicate_id: 2, object_id: 3 }),
        close: () => {},
      })),
    };
    const requireFn = vi.fn().mockReturnValue(binding);
    moduleMock.createRequire.mockReturnValue(requireFn as unknown as NodeJS.Require);

    const { loadNativeCore, __setNativeCoreForTesting } = await import(
      '../../../src/native/core.js'
    );
    __setNativeCoreForTesting(undefined);
    const loaded = loadNativeCore();
    expect(loaded).toBe(binding);
    expect(requireFn).toHaveBeenCalledWith(expect.stringContaining(addonName));
  });

  it('swallows addon load errors when native layer is optional', async () => {
    const addonName = `nervusdb-node.${process.platform}.${process.arch}.node`;
    setupFileEntries(addonName);
    moduleMock.createRequire.mockReturnValue((() => {
      throw new Error('boom');
    }) as unknown as NodeJS.Require);

    const { loadNativeCore, __setNativeCoreForTesting } = await import(
      '../../../src/native/core.js'
    );
    __setNativeCoreForTesting(undefined);
    expect(loadNativeCore()).toBeNull();
  });
});
