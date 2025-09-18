import { OrderedTriple, type IndexOrder } from './tripleIndexes.js';
export interface PageMeta {
    primaryValue: number;
    offset: number;
    length: number;
    rawLength?: number;
    crc32?: number;
}
export interface PageLookup {
    order: IndexOrder;
    pages: PageMeta[];
}
export interface PagedIndexOptions {
    directory: string;
    pageSize?: number;
    compression?: CompressionOptions;
}
export declare const DEFAULT_PAGE_SIZE = 1024;
export declare class PagedIndexWriter {
    private readonly filePath;
    private readonly pageSize;
    private readonly buffers;
    private readonly pages;
    private readonly compression;
    constructor(filePath: string, options: PagedIndexOptions);
    push(triple: OrderedTriple, primary: number): void;
    finalize(): Promise<PageMeta[]>;
    private flushPage;
}
export interface PagedIndexReaderOptions {
    directory: string;
    compression: CompressionOptions;
}
export declare class PagedIndexReader {
    private readonly options;
    private readonly lookup;
    private readonly filePath;
    constructor(options: PagedIndexReaderOptions, lookup: PageLookup);
    getPrimaryValues(): number[];
    read(primaryValue: number): Promise<OrderedTriple[]>;
    readAll(): Promise<OrderedTriple[]>;
    readSync(primaryValue: number): OrderedTriple[];
    readAllSync(): OrderedTriple[];
    streamByPrimaryValue(primaryValue: number): AsyncGenerator<OrderedTriple[], void, unknown>;
    streamAll(): AsyncGenerator<OrderedTriple[], void, unknown>;
}
export declare function pageFileName(order: string): string;
export interface PagedIndexManifest {
    version: number;
    pageSize: number;
    createdAt: number;
    compression: CompressionOptions;
    tombstones?: Array<[number, number, number]>;
    epoch?: number;
    orphans?: Array<{
        order: IndexOrder;
        pages: PageMeta[];
    }>;
    lookups: PageLookup[];
}
export declare function writePagedManifest(directory: string, manifest: PagedIndexManifest): Promise<void>;
export declare function readPagedManifest(directory: string): Promise<PagedIndexManifest | null>;
export type CompressionCodec = 'none' | 'brotli';
export interface CompressionOptions {
    codec: CompressionCodec;
    level?: number;
}
//# sourceMappingURL=pagedIndex.d.ts.map