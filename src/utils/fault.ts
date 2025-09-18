let crashPoint: string | null = null;

export function setCrashPoint(point: string | null): void {
  crashPoint = point;
}

export function triggerCrash(point: string): void {
  if (crashPoint && crashPoint === point) {
    // 一次性触发并清除
    crashPoint = null;
    throw new Error(`InjectedCrash:${point}`);
  }
}
