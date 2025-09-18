let crashPoint = null;
export function setCrashPoint(point) {
    crashPoint = point;
}
export function triggerCrash(point) {
    if (crashPoint && crashPoint === point) {
        // 一次性触发并清除
        crashPoint = null;
        throw new Error(`InjectedCrash:${point}`);
    }
}
//# sourceMappingURL=fault.js.map