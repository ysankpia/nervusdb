/**
 * Reader Registry - 基于文件系统的实现
 *
 * 借鉴LSM-Tree分层思想：每个进程管理独立的reader文件，
 * 彻底避免多进程竞争单一文件的竞态条件。
 */
export interface ReaderInfo {
    pid: number;
    epoch: number;
    ts: number;
}
export interface ReaderRegistry {
    version: number;
    readers: ReaderInfo[];
}
/**
 * 添加reader到注册表
 * 为当前进程创建独立的reader文件
 */
export declare function addReader(directory: string, info: ReaderInfo): Promise<void>;
/**
 * 从注册表移除reader
 * 删除当前进程的reader文件
 */
export declare function removeReader(directory: string, pid: number): Promise<void>;
/**
 * 获取活跃的readers
 * 遍历readers目录，读取所有reader文件
 */
export declare function getActiveReaders(directory: string): Promise<ReaderInfo[]>;
/**
 * 清理指定进程的所有reader文件
 * 用于进程启动时清理可能残留的旧文件
 */
export declare function cleanupProcessReaders(directory: string, pid: number): Promise<void>;
/**
 * 清理所有过期的reader文件
 * 用于维护操作
 */
export declare function cleanupStaleReaders(directory: string, maxAge?: number): Promise<void>;
export declare function readRegistry(directory: string): Promise<ReaderRegistry>;
//# sourceMappingURL=readerRegistry.d.ts.map