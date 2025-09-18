export declare const MAGIC_HEADER: Buffer<ArrayBuffer>;
export declare const FILE_VERSION = 2;
export declare const FILE_HEADER_LENGTH = 64;
export interface SectionPointer {
    offset: number;
    length: number;
}
export interface FileLayout {
    dictionary: SectionPointer;
    triples: SectionPointer;
    indexes: SectionPointer;
    properties: SectionPointer;
}
export interface FileHeader {
    magic: Buffer;
    version: number;
    layout: FileLayout;
}
export declare function createEmptyLayout(): FileLayout;
//# sourceMappingURL=layout.d.ts.map