/**
 * 简易内存倒排索引实现（最小可用）
 * 仅用于通过类型检查与基本查询演示，不追求完整性能与特性。
 */
import type { Document, InvertedIndex, PostingEntry, PostingList } from './types.js';

type Term = string;

export class MemoryInvertedIndex implements InvertedIndex {
  private postings: Map<Term, PostingList> = new Map();
  private docCount = 0;

  addDocument(doc: Document): void {
    this.docCount++;
    // 简化：按 tokens 建立倒排
    const positionsByTerm = new Map<Term, number[]>();
    for (const token of doc.tokens) {
      const arr = positionsByTerm.get(token.value) ?? [];
      arr.push(token.position);
      positionsByTerm.set(token.value, arr);
    }

    for (const [term, positions] of positionsByTerm) {
      const list = this.postings.get(term) ?? {
        term,
        entries: [],
        documentFrequency: 0,
      };
      const entry: PostingEntry = {
        docId: doc.id,
        frequency: positions.length,
        positions,
        field: 'content',
      };
      list.entries.push(entry);
      list.documentFrequency = list.entries.length;
      this.postings.set(term, list);
    }
  }

  removeDocument(docId: string): void {
    for (const [term, list] of this.postings) {
      const next = list.entries.filter((e) => e.docId !== docId);
      if (next.length === 0) this.postings.delete(term);
      else {
        list.entries = next;
        list.documentFrequency = next.length;
      }
    }
    if (this.docCount > 0) this.docCount--;
  }

  updateDocument(doc: Document): void {
    this.removeDocument(doc.id);
    this.addDocument(doc);
  }

  search(terms: string[]): Map<string, number> {
    // 简化：按词频累加打分
    const scores = new Map<string, number>();
    for (const t of terms) {
      const list = this.postings.get(t);
      if (!list) continue;
      for (const e of list.entries) {
        scores.set(e.docId, (scores.get(e.docId) ?? 0) + e.frequency);
      }
    }
    return scores;
  }

  getPostingList(term: string): PostingList | undefined {
    return this.postings.get(term);
  }

  getDocumentCount(): number {
    return this.docCount;
  }

  // 供引擎统计使用（非接口定义）
  getStats(): { terms: number; indexSize: number } {
    // 估算索引大小（非常粗略）
    let entries = 0;
    for (const list of this.postings.values()) entries += list.entries.length;
    return { terms: this.postings.size, indexSize: entries };
  }
}
