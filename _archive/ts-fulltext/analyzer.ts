/**
 * 文本分析器实现
 *
 * 提供多语言文本分析功能，包括分词、词干提取、N-gram生成等
 */

import type { Token, TextAnalyzer } from './types.js';

// 英文停用词列表
const ENGLISH_STOP_WORDS = new Set([
  'the',
  'a',
  'an',
  'and',
  'or',
  'but',
  'in',
  'on',
  'at',
  'to',
  'for',
  'of',
  'with',
  'by',
  'is',
  'are',
  'was',
  'were',
  'be',
  'been',
  'being',
  'have',
  'has',
  'had',
  'do',
  'does',
  'did',
  'will',
  'would',
  'could',
  'should',
  'may',
  'might',
  'can',
  'this',
  'that',
  'these',
  'those',
  'i',
  'you',
  'he',
  'she',
  'it',
  'we',
  'they',
  'me',
  'him',
  'her',
  'us',
  'them',
  'my',
  'your',
  'his',
  'its',
  'our',
  'their',
  'what',
  'which',
  'who',
  'when',
  'where',
  'why',
  'how',
  'not',
  'no',
  'yes',
]);

// 中文停用词列表
const CHINESE_STOP_WORDS = new Set([
  '的',
  '了',
  '在',
  '是',
  '我',
  '有',
  '和',
  '就',
  '不',
  '人',
  '都',
  '一',
  '一个',
  '上',
  '也',
  '很',
  '到',
  '说',
  '要',
  '去',
  '你',
  '会',
  '着',
  '没有',
  '看',
  '好',
  '自己',
  '这',
  '那',
  '来',
  '可以',
  '还',
  '什么',
  '让',
  '把',
  '被',
  '从',
  '给',
  '对',
  '向',
  '以',
  '过',
  '又',
  '用',
  '就是',
  '这个',
  '那个',
  '这些',
  '那些',
  '这样',
  '那样',
  '因为',
  '所以',
  '但是',
  '如果',
  '虽然',
  '然而',
  '而且',
  '或者',
  '既然',
  '除了',
  '关于',
  '根据',
]);

/**
 * Porter词干提取算法的简化实现
 */
class PorterStemmer {
  private static readonly VOWELS = 'aeiou';

  /**
   * 提取词干
   */
  static stem(word: string): string {
    if (word.length <= 2) return word;

    word = word.toLowerCase();

    // Step 1a: 复数形式处理
    word = this.step1a(word);

    // Step 1b: 过去式和现在分词处理
    word = this.step1b(word);

    // Step 2: 后缀替换
    word = this.step2(word);

    // Step 3: 词尾清理
    word = this.step3(word);

    return word;
  }

  private static step1a(word: string): string {
    if (word.endsWith('sses')) return word.slice(0, -2);
    if (word.endsWith('ies')) return word.slice(0, -2);
    if (word.endsWith('ss')) return word;
    if (word.endsWith('s') && word.length > 3) return word.slice(0, -1);
    return word;
  }

  private static step1b(word: string): string {
    if (word.endsWith('eed')) {
      const stem = word.slice(0, -3);
      if (this.measure(stem) > 0) return stem + 'ee';
      return word;
    }

    if (word.endsWith('ed') && this.containsVowel(word.slice(0, -2))) {
      word = word.slice(0, -2);
      return this.postProcess(word);
    }

    if (word.endsWith('ing') && this.containsVowel(word.slice(0, -3))) {
      word = word.slice(0, -3);
      return this.postProcess(word);
    }

    return word;
  }

  private static step2(word: string): string {
    const suffixes = [
      ['ational', 'ate'],
      ['tional', 'tion'],
      ['enci', 'ence'],
      ['anci', 'ance'],
      ['izer', 'ize'],
      ['abli', 'able'],
      ['alli', 'al'],
      ['entli', 'ent'],
      ['eli', 'e'],
      ['ousli', 'ous'],
      ['ization', 'ize'],
      ['ation', 'ate'],
      ['ator', 'ate'],
      ['alism', 'al'],
      ['iveness', 'ive'],
      ['fulness', 'ful'],
      ['ousness', 'ous'],
      ['aliti', 'al'],
      ['iviti', 'ive'],
      ['biliti', 'ble'],
    ];

    for (const [suffix, replacement] of suffixes) {
      if (word.endsWith(suffix)) {
        const stem = word.slice(0, -suffix.length);
        if (this.measure(stem) > 0) {
          return stem + replacement;
        }
      }
    }

    return word;
  }

  private static step3(word: string): string {
    const suffixes = [
      ['icate', 'ic'],
      ['ative', ''],
      ['alize', 'al'],
      ['iciti', 'ic'],
      ['ical', 'ic'],
      ['ful', ''],
      ['ness', ''],
    ];

    for (const [suffix, replacement] of suffixes) {
      if (word.endsWith(suffix)) {
        const stem = word.slice(0, -suffix.length);
        if (this.measure(stem) > 0) {
          return stem + replacement;
        }
      }
    }

    return word;
  }

  private static postProcess(word: string): string {
    if (word.endsWith('at') || word.endsWith('bl') || word.endsWith('iz')) {
      return word + 'e';
    }

    if (
      this.endsWithDoubleCons(word) &&
      !word.endsWith('l') &&
      !word.endsWith('s') &&
      !word.endsWith('z')
    ) {
      return word.slice(0, -1);
    }

    if (this.measure(word) === 1 && this.cvc(word)) {
      return word + 'e';
    }

    return word;
  }

  private static measure(word: string): number {
    let n = 0;
    let i = 0;

    // 跳过开头的辅音
    while (i < word.length && !this.isVowel(word[i], i, word)) i++;

    // 计算 VC 模式的次数
    while (i < word.length) {
      // 跳过元音
      while (i < word.length && this.isVowel(word[i], i, word)) i++;
      if (i >= word.length) break;
      n++;

      // 跳过辅音
      while (i < word.length && !this.isVowel(word[i], i, word)) i++;
    }

    return n;
  }

  private static containsVowel(word: string): boolean {
    for (let i = 0; i < word.length; i++) {
      if (this.isVowel(word[i], i, word)) return true;
    }
    return false;
  }

  private static isVowel(char: string, index: number, word: string): boolean {
    if (this.VOWELS.includes(char)) return true;
    if (char === 'y' && index > 0 && !this.VOWELS.includes(word[index - 1])) {
      return true;
    }
    return false;
  }

  private static endsWithDoubleCons(word: string): boolean {
    if (word.length < 2) return false;
    const last = word[word.length - 1];
    const secondLast = word[word.length - 2];
    return last === secondLast && !this.isVowel(last, word.length - 1, word);
  }

  private static cvc(word: string): boolean {
    if (word.length < 3) return false;
    const len = word.length;
    return (
      !this.isVowel(word[len - 3], len - 3, word) &&
      this.isVowel(word[len - 2], len - 2, word) &&
      !this.isVowel(word[len - 1], len - 1, word) &&
      !['w', 'x', 'y'].includes(word[len - 1])
    );
  }
}

/**
 * 标准文本分析器实现
 */
export class StandardAnalyzer implements TextAnalyzer {
  private enableStemming: boolean;
  private enableStopWords: boolean;
  private ngramSize: number;

  constructor(
    options: {
      stemming?: boolean;
      stopWords?: boolean;
      ngramSize?: number;
    } = {},
  ) {
    this.enableStemming = options.stemming ?? true;
    this.enableStopWords = options.stopWords ?? true;
    this.ngramSize = options.ngramSize ?? 2;
  }

  /**
   * 分析文本，返回词元列表
   */
  analyze(text: string, language: string = 'auto'): Token[] {
    // 1. 文本标准化
    const normalized = this.normalize(text);

    // 2. 分词
    const words = this.tokenize(normalized, language);

    // 3. 小写化
    const lowercased = words.map((token, index) => ({
      ...token,
      value: token.value.toLowerCase(),
      position: index,
    }));

    // 4. 过滤停用词
    const filtered = this.enableStopWords ? this.removeStopWords(lowercased, language) : lowercased;

    // 5. 词干提取
    const stemmed = this.enableStemming ? this.applyStemming(filtered, language) : filtered;

    // 6. 生成N-gram
    const ngrams = this.generateNGramTokens(stemmed);

    return [...stemmed, ...ngrams];
  }

  /**
   * 标准化文本
   */
  normalize(text: string): string {
    return text
      .trim()
      .replace(/\s+/g, ' ') // 合并多个空格
      .replace(/[^\w\s\u4e00-\u9fa5]/g, ' ') // 移除标点符号，保留中文
      .replace(/\s+/g, ' ') // 再次合并空格
      .trim();
  }

  /**
   * 分词处理
   */
  private tokenize(text: string, language: string): Token[] {
    const tokens: Token[] = [];

    if (language === 'zh' || this.containsChinese(text)) {
      // 中文分词 - 简单实现，基于字符
      const words = this.segmentChinese(text);
      words.forEach((word, index) => {
        tokens.push({
          value: word,
          type: 'word',
          position: index,
          length: word.length,
        });
      });
    } else {
      // 英文分词
      const words = text.split(/\s+/).filter((word) => word.length > 0);
      words.forEach((word, index) => {
        tokens.push({
          value: word,
          type: 'word',
          position: index,
          length: word.length,
        });
      });
    }

    return tokens;
  }

  /**
   * 检测是否包含中文字符
   */
  private containsChinese(text: string): boolean {
    return /[\u4e00-\u9fa5]/.test(text);
  }

  /**
   * 中文分词 - 简单实现
   * 实际应用中应该使用jieba等专业分词库
   */
  private segmentChinese(text: string): string[] {
    const words: string[] = [];
    let currentWord = '';

    for (let i = 0; i < text.length; i++) {
      const char = text[i];

      if (/[\u4e00-\u9fa5]/.test(char)) {
        // 中文字符
        if (currentWord && !/[\u4e00-\u9fa5]/.test(currentWord)) {
          words.push(currentWord);
          currentWord = '';
        }
        currentWord += char;

        // 简单的双字词处理
        if (currentWord.length === 2) {
          words.push(currentWord);
          currentWord = char; // 保留最后一个字符作为下一个词的开始
        }
      } else if (/[a-zA-Z0-9]/.test(char)) {
        // 英文字母或数字
        if (currentWord && /[\u4e00-\u9fa5]/.test(currentWord)) {
          words.push(currentWord);
          currentWord = '';
        }
        currentWord += char;
      } else {
        // 其他字符（空格、标点等）
        if (currentWord) {
          words.push(currentWord);
          currentWord = '';
        }
      }
    }

    if (currentWord) {
      words.push(currentWord);
    }

    return words.filter((word) => word.length > 0);
  }

  /**
   * 移除停用词
   */
  private removeStopWords(tokens: Token[], language: string): Token[] {
    const stopWords = language === 'zh' ? CHINESE_STOP_WORDS : ENGLISH_STOP_WORDS;

    return tokens.filter((token) => !stopWords.has(token.value));
  }

  /**
   * 词干提取
   */
  private applyStemming(tokens: Token[], language: string): Token[] {
    if (language === 'zh') {
      // 中文暂不进行词干提取
      return tokens;
    }

    return tokens.map((token) => ({
      ...token,
      value: PorterStemmer.stem(token.value),
    }));
  }

  /**
   * 生成N-gram词元
   */
  private generateNGramTokens(tokens: Token[]): Token[] {
    if (tokens.length < this.ngramSize) return [];

    const ngramTokens: Token[] = [];
    const words = tokens.map((t) => t.value);
    const ngrams = this.generateNGrams(words, this.ngramSize);

    ngrams.forEach((ngram, index) => {
      ngramTokens.push({
        value: ngram,
        type: 'ngram',
        position: index,
        length: ngram.length,
      });
    });

    return ngramTokens;
  }

  /**
   * 生成N-gram
   */
  generateNGrams(tokens: string[], n: number): string[] {
    if (tokens.length < n) return [];

    const ngrams: string[] = [];
    for (let i = 0; i <= tokens.length - n; i++) {
      ngrams.push(tokens.slice(i, i + n).join(' '));
    }

    return ngrams;
  }
}

/**
 * 关键词分析器 - 不进行任何分析，保持原文
 */
export class KeywordAnalyzer implements TextAnalyzer {
  analyze(text: string): Token[] {
    const normalized = this.normalize(text);
    return [
      {
        value: normalized,
        type: 'word',
        position: 0,
        length: normalized.length,
      },
    ];
  }

  normalize(text: string): string {
    return text.trim();
  }

  generateNGrams(tokens: string[], n: number): string[] {
    void tokens;
    void n;
    return []; // 关键词分析器不生成N-gram
  }
}

/**
 * N-gram分析器 - 专门用于生成N-gram
 */
export class NGramAnalyzer implements TextAnalyzer {
  private ngramSize: number;

  constructor(ngramSize: number = 3) {
    this.ngramSize = ngramSize;
  }

  analyze(text: string): Token[] {
    const normalized = this.normalize(text);
    const chars = Array.from(normalized); // 支持Unicode字符
    const ngrams = this.generateCharNGrams(chars, this.ngramSize);

    return ngrams.map((ngram, index) => ({
      value: ngram,
      type: 'ngram',
      position: index,
      length: ngram.length,
    }));
  }

  normalize(text: string): string {
    return text.toLowerCase().replace(/\s+/g, '');
  }

  generateNGrams(tokens: string[], n: number): string[] {
    return this.generateCharNGrams(tokens, n);
  }

  private generateCharNGrams(chars: string[], n: number): string[] {
    if (chars.length < n) return [chars.join('')];

    const ngrams: string[] = [];
    for (let i = 0; i <= chars.length - n; i++) {
      ngrams.push(chars.slice(i, i + n).join(''));
    }

    return ngrams;
  }
}

/**
 * 分析器工厂
 */
export class AnalyzerFactory {
  static createAnalyzer(
    type: 'standard' | 'keyword' | 'ngram',
    options?: { stemming?: boolean; stopWords?: boolean; ngramSize?: number },
  ): TextAnalyzer {
    switch (type) {
      case 'standard':
        return new StandardAnalyzer(options);
      case 'keyword':
        return new KeywordAnalyzer();
      case 'ngram':
        return new NGramAnalyzer(options?.ngramSize);
      default:
        // 类型已穷尽
        throw new Error('Unknown analyzer type');
    }
  }
}
