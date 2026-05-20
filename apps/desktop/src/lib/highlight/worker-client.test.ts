import { describe, expect, it, vi } from 'vitest';

import { HighlightWorkerClient } from '$lib/highlight/worker-client';

const mockedCache = vi.hoisted(() => ({
  readTokenCache: vi.fn(async () => null),
  writeTokenCache: vi.fn(async () => undefined)
}));

vi.mock('$lib/highlight/cache', () => ({
  readTokenCache: mockedCache.readTokenCache,
  writeTokenCache: mockedCache.writeTokenCache
}));

class FakeWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  postCount = 0;

  postMessage(request: {
    id: number;
    language: string;
    contentHash: string;
    rangeStart: number;
    rangeEnd: number;
  }): void {
    this.postCount += 1;
    const lines = [];
    for (let line = request.rangeStart; line <= request.rangeEnd; line += 1) {
      lines.push({
        line,
        tokens: [{ start: 0, end: 4, kind: 'keyword' }]
      });
    }
    this.onmessage?.({
      data: {
        id: request.id,
        ok: true,
        payload: {
          language: request.language,
          contentHash: request.contentHash,
          lines
        }
      }
    } as MessageEvent);
  }
}

describe('HighlightWorkerClient', () => {
  it('requests worker once and reuses cached window tokens', async () => {
    const worker = new FakeWorker();
    const client = new HighlightWorkerClient(worker as unknown as Worker);

    const first = await client.requestTokens('typescript', 'const x = 1;\nconst y = 2;', 1, 2);
    const second = await client.requestTokens('typescript', 'const x = 1;\nconst y = 2;', 1, 2);

    expect(first.get(1)?.[0]?.kind).toBe('keyword');
    expect(second.get(2)?.[0]?.kind).toBe('keyword');
    expect(worker.postCount).toBe(1);
    expect(mockedCache.writeTokenCache).toHaveBeenCalled();
  });
});
