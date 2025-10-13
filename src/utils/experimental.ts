const warned = new Set<string>();

/**
 * 标记实验性功能使用情况，并提醒开发者谨慎依赖。
 */
export function warnExperimental(feature: string): void {
  if (warned.has(feature)) {
    return;
  }
  warned.add(feature);
  try {
    // 统一输出格式，避免被误当作错误日志。
    console.warn(
      `[NervusDB][EXPERIMENTAL] 功能「${feature}」仍处于实验阶段，未来版本可能发生重大调整。`,
    );
  } catch {
    // 环境禁止 stdout 时静默失败
  }
}

/**
 * 封装一次性实验性警告，便于包裹工厂函数。
 */
export function wrapExperimental<TArgs extends unknown[], TResult>(
  feature: string,
  factory: (...args: TArgs) => TResult,
): (...args: TArgs) => TResult {
  return ((...args: TArgs): TResult => {
    warnExperimental(feature);
    // 保持原始签名与返回类型
    return factory(...args);
  }) as (...args: TArgs) => TResult;
}
