import { Language, Parser } from 'web-tree-sitter';

import type { HighlightLineTokens } from '$lib/highlight/types';

type WorkerRequest = {
  id: number;
  language: string;
  contentHash: string;
  content: string;
  rangeStart: number;
  rangeEnd: number;
};

const parserByLanguage = new Map<string, Parser>();
const languageByName = new Map<string, Language>();

let parserInitPromise: Promise<void> | null = null;

function resolveGrammar(language: string): string | null {
  const normalized = language.toLowerCase();
  if (normalized === 'rust' || normalized === 'rs') {
    return 'tree-sitter-rust.wasm';
  }
  if (normalized === 'typescript' || normalized === 'ts' || normalized === 'tsx') {
    return 'tree-sitter-typescript.wasm';
  }
  if (normalized === 'javascript' || normalized === 'js' || normalized === 'jsx') {
    return 'tree-sitter-javascript.wasm';
  }
  if (normalized === 'json') {
    return 'tree-sitter-json.wasm';
  }
  if (normalized === 'yaml' || normalized === 'yml') {
    return 'tree-sitter-yaml.wasm';
  }
  if (normalized === 'go') {
    return 'tree-sitter-go.wasm';
  }
  if (normalized === 'python' || normalized === 'py') {
    return 'tree-sitter-python.wasm';
  }
  if (normalized === 'markdown' || normalized === 'md') {
    return 'tree-sitter-markdown.wasm';
  }
  return null;
}

async function ensureParser(language: string): Promise<Parser | null> {
  const grammar = resolveGrammar(language);
  if (!grammar) {
    return null;
  }
  if (!parserInitPromise) {
    parserInitPromise = Parser.init();
  }
  await parserInitPromise;
  if (!languageByName.has(grammar)) {
    const lang = await Language.load(`/assets/grammars/${grammar}`);
    languageByName.set(grammar, lang);
  }
  if (!parserByLanguage.has(grammar)) {
    const parser = new Parser();
    parser.setLanguage(languageByName.get(grammar)!);
    parserByLanguage.set(grammar, parser);
  }
  return parserByLanguage.get(grammar)!;
}

function collectTokens(line: string): { start: number; end: number; kind: string }[] {
  const tokens: { start: number; end: number; kind: string }[] = [];
  const patterns: Array<{ regex: RegExp; kind: string }> = [
    {
      regex: /\b(const|let|fn|function|if|else|for|while|return|pub|struct|class|import|export)\b/g,
      kind: 'keyword'
    },
    { regex: /"(?:[^"\\]|\\.)*"/g, kind: 'string' },
    { regex: /'(?:[^'\\]|\\.)*'/g, kind: 'string' },
    { regex: /\b\d+\b/g, kind: 'number' },
    { regex: /(\/\/|#).*/g, kind: 'comment' }
  ];
  for (const pattern of patterns) {
    for (const match of line.matchAll(pattern.regex)) {
      if (typeof match.index !== 'number') {
        continue;
      }
      tokens.push({
        start: match.index,
        end: match.index + match[0].length,
        kind: pattern.kind
      });
    }
  }
  return tokens.sort((left, right) => left.start - right.start);
}

function tokenizeWindow(content: string, start: number, end: number): HighlightLineTokens[] {
  const lines = content.split('\n');
  const limitedStart = Math.max(1, start);
  const limitedEnd = Math.min(lines.length, end);
  const result: HighlightLineTokens[] = [];
  for (let lineNumber = limitedStart; lineNumber <= limitedEnd; lineNumber += 1) {
    const sourceLine = lines[lineNumber - 1] ?? '';
    result.push({
      line: lineNumber,
      tokens: collectTokens(sourceLine)
    });
  }
  return result;
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  try {
    const parser = await ensureParser(request.language);
    if (parser) {
      parser.parse(request.content);
    }
    const windowStart = Math.max(1, request.rangeStart - 40);
    const windowEnd = request.rangeEnd + 40;
    const lines = tokenizeWindow(request.content, windowStart, windowEnd);
    self.postMessage({
      id: request.id,
      ok: true,
      payload: {
        language: request.language,
        contentHash: request.contentHash,
        lines
      }
    });
  } catch (error) {
    self.postMessage({
      id: request.id,
      ok: false,
      error: error instanceof Error ? error.message : 'Highlight worker failure'
    });
  }
};
