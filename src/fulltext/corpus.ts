/**
 * 简易内存文档语料库实现
 */
import type { Document, DocumentCorpus } from './types.js';
import type { MemoryInvertedIndex } from './invertedIndex.js';

export class MemoryDocumentCorpus implements DocumentCorpus {
  private docs = new Map<string, Document>();
  constructor(private index: MemoryInvertedIndex) {}

  get totalDocuments(): number {
    return this.docs.size;
  }

  get averageDocumentLength(): number {
    if (this.docs.size === 0) return 0;
    let total = 0;
    for (const d of this.docs.values()) total += d.tokens.length;
    return total / this.docs.size;
  }

  addDocument(doc: Document): void {
    this.docs.set(doc.id, doc);
  }

  getDocumentsContaining(term: string): Document[] {
    const list = this.index.getPostingList(term);
    if (!list) return [];
    const res: Document[] = [];
    for (const e of list.entries) {
      const d = this.docs.get(e.docId);
      if (d) res.push(d);
    }
    return res;
  }

  getDocument(docId: string): Document | undefined {
    return this.docs.get(docId);
  }
}
