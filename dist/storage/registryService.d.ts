#!/usr/bin/env node
/**
 * Registry Service - 独立进程，管理reader注册表
 *
 * 通过Unix Socket提供IPC服务，串行化所有reader registry操作，
 * 从根本上消除read-modify-write竞态条件。
 */
declare class RegistryService {
    private server;
    private socketPath;
    constructor(socketPath: string);
    start(): Promise<void>;
    stop(): Promise<void>;
    private handleClient;
    private processCommand;
    private executeCommand;
    private readRegistry;
    private writeRegistry;
    private addReader;
    private removeReader;
    private getActiveReaders;
}
export { RegistryService };
//# sourceMappingURL=registryService.d.ts.map