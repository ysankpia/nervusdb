import type { NervusDB } from '../synapseDb.js';
import { PersistentStore } from '../core/storage/persistentStore.js';

/**
 * 插件基础接口
 *
 * 所有NervusDB插件都必须实现这个接口。
 * "好品味"原则：简单的接口，明确的职责分离。
 */
export interface NervusDBPlugin {
  /** 插件名称 */
  readonly name: string;

  /** 插件版本 */
  readonly version: string;

  /** 初始化插件 */
  initialize(db: NervusDB, store: PersistentStore): Promise<void> | void;

  /** 清理插件资源 */
  cleanup?(): Promise<void> | void;
}

/**
 * 插件管理器
 *
 * 负责插件的注册、初始化和生命周期管理。
 */
export class PluginManager {
  private plugins = new Map<string, NervusDBPlugin>();
  private initialized = false;

  constructor(
    private readonly db: NervusDB,
    private readonly store: PersistentStore,
  ) {}

  /**
   * 注册插件
   */
  register(plugin: NervusDBPlugin): void {
    if (this.initialized) {
      throw new Error('无法在初始化后注册插件');
    }

    if (this.plugins.has(plugin.name)) {
      throw new Error(`插件 ${plugin.name} 已存在`);
    }

    this.plugins.set(plugin.name, plugin);
  }

  /**
   * 初始化所有插件
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      return;
    }

    for (const plugin of this.plugins.values()) {
      await plugin.initialize(this.db, this.store);
    }

    this.initialized = true;
  }

  /**
   * 获取插件
   */
  get<T extends NervusDBPlugin>(name: string): T | undefined {
    return this.plugins.get(name) as T | undefined;
  }

  /**
   * 检查插件是否存在
   */
  has(name: string): boolean {
    return this.plugins.has(name);
  }

  /**
   * 清理所有插件
   */
  async cleanup(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    const cleanupPromises: Promise<void>[] = [];
    for (const plugin of this.plugins.values()) {
      if (plugin.cleanup) {
        cleanupPromises.push(Promise.resolve(plugin.cleanup()));
      }
    }

    await Promise.all(cleanupPromises);
    this.initialized = false;
  }

  /**
   * 列出所有已注册的插件
   */
  list(): Array<{ name: string; version: string }> {
    return Array.from(this.plugins.values()).map((plugin) => ({
      name: plugin.name,
      version: plugin.version,
    }));
  }
}
