/**
 * 性能基准测试类型定义
 *
 * 为SynapseDB的各个模块提供统一的性能测试框架
 */

/**
 * 基准测试结果
 */
export interface BenchmarkResult {
  /** 测试名称 */
  name: string;
  /** 测试描述 */
  description: string;
  /** 执行时间（毫秒） */
  executionTime: number;
  /** 内存使用量（字节） */
  memoryUsage: number;
  /** 操作次数 */
  operations: number;
  /** 每秒操作数 */
  operationsPerSecond: number;
  /** 平均延迟（毫秒） */
  averageLatency: number;
  /** 最小延迟（毫秒） */
  minLatency: number;
  /** 最大延迟（毫秒） */
  maxLatency: number;
  /** 95百分位延迟（毫秒） */
  p95Latency: number;
  /** 99百分位延迟（毫秒） */
  p99Latency: number;
  /** 测试数据量 */
  dataSize: number;
  /** 测试时间戳 */
  timestamp: Date;
  /** 额外指标 */
  metrics?: Record<string, number>;
}

/**
 * 基准测试配置
 */
export interface BenchmarkConfig {
  /** 预热次数 */
  warmupRuns?: number;
  /** 测试次数 */
  runs?: number;
  /** 超时时间（毫秒） */
  timeout?: number;
  /** 是否收集内存使用情况 */
  collectMemoryUsage?: boolean;
  /** 是否收集延迟统计 */
  collectLatencyStats?: boolean;
  /** 数据生成配置 */
  dataGeneration?: DataGenerationConfig;
}

/**
 * 数据生成配置
 */
export interface DataGenerationConfig {
  /** 数据集大小 */
  size: number;
  /** 数据类型 */
  type: 'facts' | 'nodes' | 'edges' | 'documents' | 'coordinates';
  /** 随机种子 */
  seed?: number;
  /** 特定参数 */
  params?: Record<string, any>;
}

/**
 * 基准测试套件
 */
export interface BenchmarkSuite {
  /** 套件名称 */
  name: string;
  /** 套件描述 */
  description: string;
  /** 基准测试列表 */
  benchmarks: BenchmarkTest[];
  /** 套件配置 */
  config?: BenchmarkConfig;
}

/**
 * 基准测试接口
 */
export interface BenchmarkTest {
  /** 测试名称 */
  name: string;
  /** 测试描述 */
  description: string;
  /** 测试函数 */
  test: (config: BenchmarkConfig) => Promise<BenchmarkResult> | BenchmarkResult;
  /** 测试配置 */
  config?: BenchmarkConfig;
  /** 前置条件 */
  setup?: (config: BenchmarkConfig) => Promise<void> | void;
  /** 清理函数 */
  teardown?: (config: BenchmarkConfig) => Promise<void> | void;
}

/**
 * 性能回归检测配置
 */
export interface RegressionConfig {
  /** 基线结果文件路径 */
  baselinePath?: string;
  /** 允许的性能退化阈值（百分比） */
  regressionThreshold?: number;
  /** 需要检查的指标 */
  metricsToCheck?: Array<keyof BenchmarkResult>;
  /** 是否自动更新基线 */
  autoUpdateBaseline?: boolean;
}

/**
 * 回归检测结果
 */
export interface RegressionResult {
  /** 测试名称 */
  testName: string;
  /** 是否通过 */
  passed: boolean;
  /** 当前值 */
  currentValue: number;
  /** 基线值 */
  baselineValue: number;
  /** 变化百分比 */
  changePercent: number;
  /** 指标名称 */
  metric: string;
  /** 详细信息 */
  details?: string;
}

/**
 * 基准测试报告
 */
export interface BenchmarkReport {
  /** 报告生成时间 */
  timestamp: Date;
  /** 测试环境信息 */
  environment: EnvironmentInfo;
  /** 测试结果 */
  results: BenchmarkResult[];
  /** 回归检测结果 */
  regressions?: RegressionResult[];
  /** 摘要统计 */
  summary: BenchmarkSummary;
}

/**
 * 环境信息
 */
export interface EnvironmentInfo {
  /** Node.js版本 */
  nodeVersion: string;
  /** 操作系统 */
  platform: string;
  /** CPU架构 */
  arch: string;
  /** 内存总量 */
  totalMemory: number;
  /** CPU核心数 */
  cpuCores: number;
  /** 测试时间 */
  timestamp: Date;
}

/**
 * 基准测试摘要
 */
export interface BenchmarkSummary {
  /** 总测试数 */
  totalTests: number;
  /** 通过的测试数 */
  passedTests: number;
  /** 失败的测试数 */
  failedTests: number;
  /** 总执行时间 */
  totalExecutionTime: number;
  /** 最快测试 */
  fastestTest: string;
  /** 最慢测试 */
  slowestTest: string;
  /** 平均执行时间 */
  averageExecutionTime: number;
  /** 内存使用峰值 */
  peakMemoryUsage: number;
}

/**
 * 性能监控器接口
 */
export interface PerformanceMonitor {
  /** 开始监控 */
  start(): void;
  /** 停止监控 */
  stop(): PerformanceMetrics;
  /** 重置监控器 */
  reset(): void;
}

/**
 * 性能指标
 */
export interface PerformanceMetrics {
  /** 开始时间 */
  startTime: number;
  /** 结束时间 */
  endTime: number;
  /** 执行时间 */
  executionTime: number;
  /** 开始内存使用量 */
  startMemory: number;
  /** 结束内存使用量 */
  endMemory: number;
  /** 内存使用量差值 */
  memoryDelta: number;
  /** 峰值内存使用量 */
  peakMemory: number;
}

/**
 * 数据生成器接口
 */
export interface DataGenerator<T> {
  /** 生成测试数据 */
  generate(config: DataGenerationConfig): T[];
  /** 生成单个数据项 */
  generateSingle(params?: Record<string, any>): T;
}

/**
 * 基准测试运行器接口
 */
export interface BenchmarkRunner {
  /** 运行单个测试 */
  runTest(test: BenchmarkTest, config?: BenchmarkConfig): Promise<BenchmarkResult>;
  /** 运行测试套件 */
  runSuite(suite: BenchmarkSuite): Promise<BenchmarkResult[]>;
  /** 运行所有测试 */
  runAll(suites: BenchmarkSuite[]): Promise<BenchmarkReport>;
}

/**
 * 基准测试比较器接口
 */
export interface BenchmarkComparator {
  /** 比较两个结果 */
  compare(current: BenchmarkResult, baseline: BenchmarkResult): RegressionResult[];
  /** 检查性能回归 */
  checkRegression(results: BenchmarkResult[], config: RegressionConfig): RegressionResult[];
}

/**
 * 基准测试报告器接口
 */
export interface BenchmarkReporter {
  /** 生成控制台报告 */
  generateConsoleReport(report: BenchmarkReport): string;
  /** 生成JSON报告 */
  generateJSONReport(report: BenchmarkReport): string;
  /** 生成HTML报告 */
  generateHTMLReport(report: BenchmarkReport): string;
  /** 生成CSV报告 */
  generateCSVReport(report: BenchmarkReport): string;
}

/**
 * 负载测试配置
 */
export interface LoadTestConfig {
  /** 并发用户数 */
  concurrentUsers: number;
  /** 测试持续时间（秒） */
  duration: number;
  /** 请求率（每秒请求数） */
  requestRate?: number;
  /** 逐步增加负载 */
  rampUp?: {
    /** 增加持续时间（秒） */
    duration: number;
    /** 起始用户数 */
    startUsers: number;
    /** 结束用户数 */
    endUsers: number;
  };
}

/**
 * 负载测试结果
 */
export interface LoadTestResult {
  /** 总请求数 */
  totalRequests: number;
  /** 成功请求数 */
  successfulRequests: number;
  /** 失败请求数 */
  failedRequests: number;
  /** 请求成功率 */
  successRate: number;
  /** 平均响应时间 */
  averageResponseTime: number;
  /** 最小响应时间 */
  minResponseTime: number;
  /** 最大响应时间 */
  maxResponseTime: number;
  /** 吞吐量（每秒请求数） */
  throughput: number;
  /** 响应时间分布 */
  responseTimeDistribution: {
    p50: number;
    p90: number;
    p95: number;
    p99: number;
  };
  /** 错误分布 */
  errorDistribution: Record<string, number>;
}

/**
 * 内存泄漏检测配置
 */
export interface MemoryLeakConfig {
  /** 测试迭代次数 */
  iterations: number;
  /** 每次迭代的操作数 */
  operationsPerIteration: number;
  /** 内存增长阈值（字节） */
  memoryGrowthThreshold: number;
  /** 强制垃圾回收 */
  forceGC: boolean;
}

/**
 * 内存泄漏检测结果
 */
export interface MemoryLeakResult {
  /** 是否检测到内存泄漏 */
  hasLeak: boolean;
  /** 初始内存使用量 */
  initialMemory: number;
  /** 最终内存使用量 */
  finalMemory: number;
  /** 内存增长量 */
  memoryGrowth: number;
  /** 每次迭代的内存使用情况 */
  memoryProgression: number[];
  /** 内存增长趋势 */
  growthTrend: 'increasing' | 'stable' | 'decreasing';
}

/**
 * CPU性能分析配置
 */
export interface CPUProfilingConfig {
  /** 采样频率（Hz） */
  sampleRate?: number;
  /** 分析持续时间（毫秒） */
  duration?: number;
  /** 包含的函数模式 */
  includePatterns?: string[];
  /** 排除的函数模式 */
  excludePatterns?: string[];
}

/**
 * CPU性能分析结果
 */
export interface CPUProfilingResult {
  /** 函数调用统计 */
  functionStats: Array<{
    /** 函数名 */
    name: string;
    /** 调用次数 */
    callCount: number;
    /** 总时间 */
    totalTime: number;
    /** 平均时间 */
    averageTime: number;
    /** 时间占比 */
    percentage: number;
  }>;
  /** 热点函数（Top N） */
  hotSpots: string[];
  /** 调用栈深度统计 */
  stackDepthStats: {
    average: number;
    max: number;
    min: number;
  };
}
