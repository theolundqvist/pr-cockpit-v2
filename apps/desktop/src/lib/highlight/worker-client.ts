import { readTokenCache, writeTokenCache } from '$lib/highlight/cache';
import type { HighlightLineTokens, HighlightResponse, HighlightToken } from '$lib/highlight/types';

type WorkerRequest = {
  id: number;
  language: string;
  contentHash: string;
  content: string;
  rangeStart: number;
  rangeEnd: number;
};

type WorkerSuccess = {
  id: number;
  ok: true;
  payload: HighlightResponse;
};

type WorkerFailure = {
  id: number;
  ok: false;
  error: string;
};

type WorkerMessage = WorkerSuccess | WorkerFailure;

function mapFromLines(lines: HighlightLineTokens[]): Map<number, HighlightToken[]> {
  const byLine = new Map<number, HighlightToken[]>();
  for (const entry of lines) {
    byLine.set(entry.line, entry.tokens);
  }
  return byLine;
}

function linesFromMap(byLine: Map<number, HighlightToken[]>): HighlightLineTokens[] {
  return Array.from(byLine.entries()).map(([line, tokens]) => ({ line, tokens }));
}

async function sha256(input: string): Promise<string> {
  if (typeof crypto === 'undefined' || !crypto.subtle) {
    return `${input.length}`;
  }
  const encoded = new TextEncoder().encode(input);
  const digest = await crypto.subtle.digest('SHA-256', encoded);
  const bytes = Array.from(new Uint8Array(digest));
  return bytes.map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

export class HighlightWorkerClient {
  private worker: Worker;
  private requestId = 0;
  private pending = new Map<
    number,
    { resolve: (value: HighlightResponse) => void; reject: (reason: Error) => void }
  >();
  private memoryCache = new Map<string, Map<number, HighlightToken[]>>();

  constructor(worker?: Worker) {
    this.worker =
      worker ??
      new Worker(new URL('../workers/highlight.ts', import.meta.url), {
        type: 'module'
      });
    this.worker.onmessage = (event: MessageEvent<WorkerMessage>) => {
      const message = event.data;
      const pending = this.pending.get(message.id);
      if (!pending) {
        return;
      }
      this.pending.delete(message.id);
      if (message.ok) {
        pending.resolve(message.payload);
      } else {
        pending.reject(new Error(message.error));
      }
    };
  }

  async requestTokens(
    language: string,
    content: string,
    rangeStart: number,
    rangeEnd: number
  ): Promise<Map<number, HighlightToken[]>> {
    const contentHash = await sha256(content);
    const key = `${language}:${contentHash}`;
    if (!this.memoryCache.has(key)) {
      const cached = await readTokenCache(language, contentHash);
      this.memoryCache.set(key, mapFromLines(cached ?? []));
    }

    const memory = this.memoryCache.get(key)!;
    const wanted = new Map<number, HighlightToken[]>();
    let missing = false;
    for (let line = rangeStart; line <= rangeEnd; line += 1) {
      const tokens = memory.get(line);
      if (tokens) {
        wanted.set(line, tokens);
      } else {
        missing = true;
      }
    }
    if (!missing) {
      return wanted;
    }

    const payload = await this.post({
      id: this.nextId(),
      language,
      contentHash,
      content,
      rangeStart,
      rangeEnd
    });

    for (const line of payload.lines) {
      memory.set(line.line, line.tokens);
    }
    await writeTokenCache(language, contentHash, linesFromMap(memory));

    for (let line = rangeStart; line <= rangeEnd; line += 1) {
      wanted.set(line, memory.get(line) ?? []);
    }
    return wanted;
  }

  private nextId(): number {
    this.requestId += 1;
    return this.requestId;
  }

  private post(request: WorkerRequest): Promise<HighlightResponse> {
    return new Promise((resolve, reject) => {
      this.pending.set(request.id, { resolve, reject });
      this.worker.postMessage(request);
    });
  }
}
