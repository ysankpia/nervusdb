/**
 * NervusDB 数据库打开选项
 *
 * 这些选项控制数据库的行为、性能和并发特性。
 */
export interface NervusDBOpenOptions {
  /**
   * 索引目录路径
   *
   * 如果未指定，将使用 `${dbPath}.pages` 作为默认目录。
   * 索引目录包含分页索引文件、manifest 和相关元数据。
   *
   * @default `${dbPath}.pages`
   * @example '/path/to/database.synapsedb.pages'
   */
  indexDirectory?: string;

  /**
   * 页面大小（三元组数量）
   *
   * 控制每个索引页面包含的最大三元组数量。较小的页面减少内存使用但增加查询开销；
   * 较大的页面提高查询性能但增加内存使用。
   *
   * @default 1000
   * @minimum 1
   * @maximum 10000
   * @example 2000
   */
  pageSize?: number;

  /**
   * 是否重建索引
   *
   * 当设为 true 时，打开数据库时将丢弃现有的分页索引并从头重建。
   * 用于索引损坏恢复或格式升级。
   *
   * @default false
   * @warning 重建索引会导致启动时间显著增加
   */
  rebuildIndexes?: boolean;

  /**
   * 压缩选项
   *
   * 控制索引页面的压缩方式。压缩可以减少磁盘使用但增加 CPU 开销。
   *
   * @default { codec: 'none' }
   */
  compression?: {
    /** 压缩算法 */
    codec: 'none' | 'brotli';
    /** 压缩级别 (1-11 for brotli) */
    level?: number;
  };

  /**
   * 启用进程级独占写锁
   *
   * 当启用时，同一路径只允许一个写者进程访问。防止多个进程同时写入导致的数据损坏。
   * 读者不受此锁限制。
   *
   * @default false
   * @recommended true（生产环境）
   * @warning 禁用锁可能导致并发写入时的数据损坏
   */
  enableLock?: boolean;

  /**
   * 注册为读者
   *
   * 当启用时，此实例将在读者注册表中注册，允许维护任务（压缩、GC）
   * 检测活跃读者并避免影响正在进行的查询。
   *
   * @default true（自 v2 起）
   * @note 设为 false 可能导致维护任务与查询冲突
   */
  registerReader?: boolean;

  /**
   * 暂存模式
   *
   * 控制写入策略。'lsm-lite' 模式使用 LSM 风格的暂存层，
   * 可以提高写入性能但增加复杂性。
   *
   * @default 'default'
   * @experimental 'lsm-lite' 模式仍在实验阶段
   */
  stagingMode?: 'default' | 'lsm-lite';

  /**
   * 启用跨周期 txId 幂等去重
   *
   * 当启用时，系统将持久化事务 ID 以支持跨数据库重启的幂等性。
   * 适用于需要精确一次执行语义的场景。
   *
   * @default false
   * @note 启用会略微增加存储开销和启动时间
   */
  enablePersistentTxDedupe?: boolean;

  /**
   * 记忆的最大事务 ID 数量
   *
   * 控制内存中保持的事务 ID 数量，用于幂等性检查。
   * 较大的值提供更长的幂等窗口但使用更多内存。
   *
   * @default 1000
   * @minimum 100
   * @maximum 100000
   */
  maxRememberTxIds?: number;

  /**
   * 实验性功能开关
   *
   * 这些能力尚未稳定，默认关闭。建议仅在评估阶段显式开启。
   * 可通过环境变量 `SYNAPSEDB_ENABLE_EXPERIMENTAL_QUERIES=1` 全局开启查询语言前端。
   */
  experimental?: {
    /** 是否启用 Cypher 查询语言插件 */
    cypher?: boolean;
    /** 是否启用 Gremlin 查询语言辅助工厂 */
    gremlin?: boolean;
    /** 是否启用 GraphQL 查询语言辅助工厂 */
    graphql?: boolean;
  };
}

/**
 * 批次提交选项
 */
export interface CommitBatchOptions {
  /**
   * 持久性保证
   *
   * 当设为 true 时，提交操作将强制同步到磁盘（fsync），
   * 确保在系统崩溃后数据不会丢失。
   *
   * @default false
   * @note 启用会显著降低写入性能但提供更强的持久性保证
   */
  durable?: boolean;
}

/**
 * 批次开始选项
 */
export interface BeginBatchOptions {
  /**
   * 事务 ID
   *
   * 可选的事务标识符，用于幂等性控制。相同 txId 的事务
   * 只会执行一次，重复提交将被忽略。
   *
   * @example 'tx-2024-001'
   */
  txId?: string;

  /**
   * 会话 ID
   *
   * 可选的会话标识符，用于审计和调试。
   *
   * @example 'session-user-123'
   */
  sessionId?: string;
}

/**
 * 判断输入是否符合 NervusDB 打开选项的基本约束
 */
export function isNervusDBOpenOptions(value: unknown): value is NervusDBOpenOptions {
  if (value === null || typeof value !== 'object') {
    return false;
  }

  const options = value as Record<string, unknown>;

  const ensureOptionalBoolean = (key: keyof NervusDBOpenOptions): boolean => {
    if (!(key in options)) return true;
    return typeof options[key] === 'boolean';
  };

  const ensureOptionalNumber = (
    key: keyof NervusDBOpenOptions,
    { min, max, integer }: { min?: number; max?: number; integer?: boolean } = {},
  ): boolean => {
    if (!(key in options)) return true;
    const candidate = options[key];
    if (typeof candidate !== 'number' || !Number.isFinite(candidate)) return false;
    if (integer && !Number.isInteger(candidate)) return false;
    if (min !== undefined && candidate < min) return false;
    if (max !== undefined && candidate > max) return false;
    return true;
  };

  if ('indexDirectory' in options && typeof options.indexDirectory !== 'string') {
    return false;
  }

  if (!ensureOptionalNumber('pageSize', { min: 1, max: 10000, integer: true })) {
    return false;
  }

  if (!ensureOptionalBoolean('rebuildIndexes')) {
    return false;
  }

  if ('compression' in options) {
    const compression = options.compression;
    if (compression === null || typeof compression !== 'object') {
      return false;
    }

    const { codec, level } = compression as { codec?: unknown; level?: unknown };
    if (codec !== 'none' && codec !== 'brotli') {
      return false;
    }

    if (
      level !== undefined &&
      (typeof level !== 'number' || !Number.isFinite(level) || level < 1 || level > 11)
    ) {
      return false;
    }
  }

  if (!ensureOptionalBoolean('enableLock')) {
    return false;
  }

  if (!ensureOptionalBoolean('registerReader')) {
    return false;
  }

  if (
    'stagingMode' in options &&
    options.stagingMode !== 'default' &&
    options.stagingMode !== 'lsm-lite'
  ) {
    return false;
  }

  if (!ensureOptionalBoolean('enablePersistentTxDedupe')) {
    return false;
  }

  if (!ensureOptionalNumber('maxRememberTxIds', { min: 100, max: 100000, integer: true })) {
    return false;
  }

  if ('experimental' in options) {
    const experimental = options.experimental;
    if (experimental !== undefined) {
      if (experimental === null || typeof experimental !== 'object') {
        return false;
      }
      const expRecord = experimental as Record<string, unknown>;
      for (const key of ['cypher', 'gremlin', 'graphql'] as const) {
        if (key in expRecord && typeof expRecord[key] !== 'boolean') {
          return false;
        }
      }
    }
  }

  return true;
}

/**
 * 断言输入符合 NervusDB 打开选项要求
 */
export function assertNervusDBOpenOptions(
  value: unknown,
  message?: string,
): asserts value is NervusDBOpenOptions {
  if (!isNervusDBOpenOptions(value)) {
    throw new TypeError(message ?? 'NervusDB 打开选项格式错误');
  }
}
