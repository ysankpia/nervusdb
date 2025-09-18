/**
 * Registry Service Manager for Tests
 *
 * 按测试文件粒度管理Registry Service进程的生命周期
 */
import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { setRegistrySocketPath, generateTestSocketPath, pingRegistryService } from '../storage/readerRegistry';
export class RegistryServiceManager {
    process = null;
    socketPath;
    constructor(testName) {
        // 为每个测试文件生成唯一的socket路径
        this.socketPath = testName
            ? `/tmp/synapsedb-registry-test-${testName}.sock`
            : generateTestSocketPath();
    }
    /**
     * 启动Registry Service进程
     */
    async start() {
        if (this.process) {
            throw new Error('Registry Service already started');
        }
        // 设置客户端使用的socket路径
        setRegistrySocketPath(this.socketPath);
        // 启动Registry Service子进程
        const servicePath = join(__dirname, '../storage/registryService.js');
        this.process = spawn('node', [servicePath, this.socketPath], {
            stdio: ['ignore', 'pipe', 'pipe'],
            detached: false, // 让子进程随父进程一起退出
        });
        this.process.stdout?.setEncoding('utf8');
        this.process.stderr?.setEncoding('utf8');
        // 监听进程输出（用于调试）
        this.process.stdout?.on('data', (data) => {
            if (process.env.DEBUG_REGISTRY) {
                console.log(`[Registry Service] ${data}`);
            }
        });
        this.process.stderr?.on('data', (data) => {
            console.error(`[Registry Service Error] ${data}`);
        });
        this.process.on('error', (err) => {
            console.error('Registry Service process error:', err);
        });
        this.process.on('exit', (code, signal) => {
            if (process.env.DEBUG_REGISTRY) {
                console.log(`Registry Service exited with code ${code}, signal ${signal}`);
            }
            this.process = null;
        });
        // 等待服务启动
        await this.waitForService();
    }
    /**
     * 停止Registry Service进程
     */
    async stop() {
        if (!this.process) {
            return;
        }
        const proc = this.process;
        this.process = null;
        // 优雅退出
        proc.kill('SIGTERM');
        // 等待进程退出，最多等待5秒
        return new Promise((resolve) => {
            const timeout = setTimeout(() => {
                // 强制杀死
                proc.kill('SIGKILL');
                resolve();
            }, 5000);
            proc.on('exit', () => {
                clearTimeout(timeout);
                resolve();
            });
        });
    }
    /**
     * 获取当前socket路径
     */
    getSocketPath() {
        return this.socketPath;
    }
    /**
     * 检查服务是否运行
     */
    async isRunning() {
        if (!this.process) {
            return false;
        }
        return await pingRegistryService();
    }
    /**
     * 等待服务启动
     */
    async waitForService(maxAttempts = 50) {
        for (let i = 0; i < maxAttempts; i++) {
            try {
                const isRunning = await pingRegistryService();
                if (isRunning) {
                    return;
                }
            }
            catch {
                // 忽略连接错误，继续重试
            }
            // 等待100ms后重试
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        throw new Error(`Registry Service failed to start after ${maxAttempts * 100}ms`);
    }
}
/**
 * 全局Registry Service管理器实例
 */
let globalManager = null;
/**
 * 获取或创建全局Registry Service管理器
 */
export function getGlobalRegistryManager(testName) {
    if (!globalManager) {
        globalManager = new RegistryServiceManager(testName);
    }
    return globalManager;
}
/**
 * 清理全局Registry Service管理器
 */
export async function cleanupGlobalRegistryManager() {
    if (globalManager) {
        await globalManager.stop();
        globalManager = null;
    }
}
/**
 * 为vitest测试提供的便捷hooks
 */
export function setupRegistryServiceForTest(testName) {
    let manager;
    return {
        async beforeAll() {
            manager = new RegistryServiceManager(testName);
            await manager.start();
        },
        async afterAll() {
            if (manager) {
                await manager.stop();
            }
        },
        getManager() {
            return manager;
        }
    };
}
//# sourceMappingURL=registryServiceManager.js.map