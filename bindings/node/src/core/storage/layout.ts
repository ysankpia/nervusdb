export const MAGIC_HEADER = Buffer.from('SYNAPSEDB', 'utf8');
export const FILE_VERSION = 2;
export const FILE_HEADER_LENGTH = 64;

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

export function createEmptyLayout(): FileLayout {
  return {
    dictionary: { offset: FILE_HEADER_LENGTH, length: 0 },
    triples: { offset: FILE_HEADER_LENGTH, length: 0 },
    indexes: { offset: FILE_HEADER_LENGTH, length: 0 },
    properties: { offset: FILE_HEADER_LENGTH, length: 0 },
  };
}
