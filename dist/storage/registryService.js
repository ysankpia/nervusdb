#!/usr/bin/env node
/**
 * Registry Service - 独立进程，管理reader注册表
 *
 * 通过Unix Socket提供IPC服务，串行化所有reader registry操作，
 * 从根本上消除read-modify-write竞态条件。
 */
import * as net from 'node:net';
import * as fs from 'node:fs/promises';
import { dirname, join } from 'node:path';
class RegistryService {
    server;
    socketPath;
    constructor(socketPath) {
        this.socketPath = socketPath;
        this.server = net.createServer((client) => {
            this.handleClient(client);
        });
    }
    async start() {
        // 清理可能存在的旧socket文件
        try {
            await fs.unlink(this.socketPath);
        }
        catch {
            // 忽略文件不存在的错误
        }
        return new Promise((resolve, reject) => {
            this.server.listen(this.socketPath, () => {
                console.log(`Registry service listening on ${this.socketPath}`);
                resolve();
            });
            this.server.on('error', reject);
        });
    }
    async stop() {
        return new Promise((resolve) => {
            this.server.close(() => {
                // 清理socket文件
                fs.unlink(this.socketPath).catch(() => { });
                resolve();
            });
        });
    }
    handleClient(client) {
        let buffer = '';
        client.on('data', (data) => {
            buffer += data.toString();
            // 简单的行分隔协议
            const lines = buffer.split('\n');
            buffer = lines.pop() || '';
            for (const line of lines) {
                if (line.trim()) {
                    this.processCommand(client, line.trim());
                }
            }
        });
        client.on('error', (err) => {
            console.error('Client connection error:', err);
        });
    }
    async processCommand(client, commandStr) {
        try {
            const command = JSON.parse(commandStr);
            const response = await this.executeCommand(command);
            client.write(JSON.stringify(response) + '\n');
        }
        catch (error) {
            const response = {
                success: false,
                error: error instanceof Error ? error.message : String(error)
            };
            client.write(JSON.stringify(response) + '\n');
        }
        finally {
            client.end();
        }
    }
    async executeCommand(command) {
        switch (command.action) {
            case 'ping':
                return { success: true, data: 'pong' };
            case 'add': {
                if (!command.directory || !command.payload) {
                    throw new Error('Missing directory or payload for add command');
                }
                await this.addReader(command.directory, command.payload);
                return { success: true };
            }
            case 'remove': {
                if (!command.directory || !command.payload?.pid) {
                    throw new Error('Missing directory or pid for remove command');
                }
                await this.removeReader(command.directory, command.payload.pid);
                return { success: true };
            }
            case 'list': {
                if (!command.directory) {
                    throw new Error('Missing directory for list command');
                }
                const readers = await this.getActiveReaders(command.directory);
                return { success: true, data: readers };
            }
            default:
                throw new Error(`Unknown command: ${command.action}`);
        }
    }
    // 原readerRegistry.ts的逻辑，现在在服务端执行
    async readRegistry(directory) {
        const file = join(directory, 'readers.json');
        try {
            const buf = await fs.readFile(file);
            return JSON.parse(buf.toString('utf8'));
        }
        catch {
            return { version: 1, readers: [] };
        }
    }
    async writeRegistry(directory, reg) {
        const file = join(directory, 'readers.json');
        const tmp = `${file}.tmp`;
        const json = Buffer.from(JSON.stringify(reg, null, 2), 'utf8');
        // 确保目录存在
        await fs.mkdir(dirname(file), { recursive: true });
        const fh = await fs.open(tmp, 'w');
        try {
            await fh.write(json, 0, json.length, 0);
            await fh.sync();
        }
        finally {
            await fh.close();
        }
        await fs.rename(tmp, file);
        // best-effort 同步目录元数据
        try {
            const dh = await fs.open(dirname(file), 'r');
            try {
                await dh.sync();
            }
            finally {
                await dh.close();
            }
        }
        catch {
            // 忽略目录同步失败
        }
    }
    async addReader(directory, info) {
        const reg = await this.readRegistry(directory);
        const existing = reg.readers.find((r) => r.pid === info.pid);
        if (existing) {
            existing.epoch = info.epoch;
            existing.ts = info.ts;
        }
        else {
            reg.readers.push(info);
        }
        await this.writeRegistry(directory, reg);
    }
    async removeReader(directory, pid) {
        const reg = await this.readRegistry(directory);
        reg.readers = reg.readers.filter((r) => r.pid !== pid);
        await this.writeRegistry(directory, reg);
    }
    async getActiveReaders(directory) {
        const reg = await this.readRegistry(directory);
        return reg.readers;
    }
}
// 如果直接运行此脚本
if (require.main === module) {
    const socketPath = process.argv[2] || '/tmp/synapsedb-registry.sock';
    const service = new RegistryService(socketPath);
    // 优雅退出处理
    const shutdown = async () => {
        console.log('Shutting down registry service...');
        await service.stop();
        process.exit(0);
    };
    process.on('SIGTERM', shutdown);
    process.on('SIGINT', shutdown);
    service.start().catch((err) => {
        console.error('Failed to start registry service:', err);
        process.exit(1);
    });
}
export { RegistryService };
//# sourceMappingURL=registryService.js.map