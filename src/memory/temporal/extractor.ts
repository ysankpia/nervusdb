export interface ExtractedEntity {
  kind: string;
  canonical: string;
  original: string;
  alias?: string;
}

const STOPWORDS = new Set([
  'the',
  'and',
  'with',
  'from',
  'into',
  'that',
  'this',
  'have',
  'about',
  'because',
]);

export function extractEntities(text: string): ExtractedEntity[] {
  const results = new Map<string, ExtractedEntity>();

  const hashtagRegex = /#([A-Za-z0-9_]+)/g;
  let match: RegExpExecArray | null;
  while ((match = hashtagRegex.exec(text)) !== null) {
    const canonical = match[1].toLowerCase();
    if (canonical.length < 2) continue;
    const entity: ExtractedEntity = {
      kind: 'topic',
      canonical,
      original: match[0],
      alias: match[1],
    };
    results.set(`hashtag:${canonical}`, entity);
  }

  const properNounRegex = /\b([A-Z][a-zA-Z]{2,})\b/g;
  while ((match = properNounRegex.exec(text)) !== null) {
    const candidate = match[1].toLowerCase();
    if (STOPWORDS.has(candidate)) continue;
    if (results.has(`noun:${candidate}`)) continue;
    const entity: ExtractedEntity = {
      kind: 'keyword',
      canonical: candidate,
      original: match[1],
    };
    results.set(`noun:${candidate}`, entity);
  }

  return Array.from(results.values());
}
