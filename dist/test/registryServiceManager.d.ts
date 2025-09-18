/**
 * Registry Service Manager for Tests
 *
 * 按测试文件粒度管理Registry Service进程的生命周期
 */
export declare class RegistryServiceManager {
    private process;
    private socketPath;
    constructor(testName?: string);
    /**
     * 启动Registry Service进程
     */
    start(): Promise<void>;
    /**
     * 停止Registry Service进程
     */
    stop(): Promise<void>;
    /**
     * 获取当前socket路径
     */
    getSocketPath(): string;
    /**
     * 检查服务是否运行
     */
    isRunning(): Promise<boolean>;
    /**
     * 等待服务启动
     */
    private waitForService;
}
/**
 * 获取或创建全局Registry Service管理器
 */
export declare function getGlobalRegistryManager(testName?: string): RegistryServiceManager;
/**
 * 清理全局Registry Service管理器
 */
export declare function cleanupGlobalRegistryManager(): Promise<void>;
/**
 * 为vitest测试提供的便捷hooks
 */
export declare function setupRegistryServiceForTest(testName?: string): {
    beforeAll(): Promise<void>;
    afterAll(): Promise<void>;
    getManager(): RegistryServiceManager;
};
//# sourceMappingURL=registryServiceManager.d.ts.map