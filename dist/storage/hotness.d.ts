import type { IndexOrder } from './tripleIndexes.js';
export interface HotnessData {
    version: number;
    updatedAt: number;
    counts: Record<IndexOrder, Record<string, number>>;
}
export declare function readHotness(directory: string): Promise<HotnessData>;
export declare function writeHotness(directory: string, data: HotnessData): Promise<void>;
//# sourceMappingURL=hotness.d.ts.map