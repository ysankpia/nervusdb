export const MAGIC_HEADER = Buffer.from('SYNAPSEDB', 'utf8');
export const FILE_VERSION = 2;
export const FILE_HEADER_LENGTH = 64;
export function createEmptyLayout() {
    return {
        dictionary: { offset: FILE_HEADER_LENGTH, length: 0 },
        triples: { offset: FILE_HEADER_LENGTH, length: 0 },
        indexes: { offset: FILE_HEADER_LENGTH, length: 0 },
        properties: { offset: FILE_HEADER_LENGTH, length: 0 },
    };
}
//# sourceMappingURL=layout.js.map