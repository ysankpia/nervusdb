import { promises as fs } from 'node:fs';
import { join, dirname } from 'node:path';
const FILE = 'hotness.json';
export async function readHotness(directory) {
    const file = join(directory, FILE);
    try {
        const buf = await fs.readFile(file);
        return JSON.parse(buf.toString('utf8'));
    }
    catch {
        return { version: 1, updatedAt: Date.now(), counts: { SPO: {}, SOP: {}, POS: {}, PSO: {}, OSP: {}, OPS: {} } };
    }
}
export async function writeHotness(directory, data) {
    const file = join(directory, FILE);
    const tmp = `${file}.tmp`;
    const json = Buffer.from(JSON.stringify({ ...data, updatedAt: Date.now() }, null, 2), 'utf8');
    const fh = await fs.open(tmp, 'w');
    try {
        await fh.write(json, 0, json.length, 0);
        await fh.sync();
    }
    finally {
        await fh.close();
    }
    await fs.rename(tmp, file);
    try {
        const dh = await fs.open(dirname(file), 'r');
        try {
            await dh.sync();
        }
        finally {
            await dh.close();
        }
    }
    catch { }
}
//# sourceMappingURL=hotness.js.map