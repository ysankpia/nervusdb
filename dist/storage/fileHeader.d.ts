import { FileHeader } from './layout.js';
export interface SerializedSections {
    dictionary: Buffer;
    triples: Buffer;
    indexes?: Buffer;
    properties?: Buffer;
}
export declare function writeStorageFile(path: string, sections: SerializedSections): Promise<void>;
export interface LoadedSections {
    header: FileHeader;
    dictionary: Buffer;
    triples: Buffer;
    indexes: Buffer;
    properties: Buffer;
}
export declare function readStorageFile(path: string): Promise<LoadedSections>;
export declare function initializeIfMissing(path: string): Promise<void>;
//# sourceMappingURL=fileHeader.d.ts.map