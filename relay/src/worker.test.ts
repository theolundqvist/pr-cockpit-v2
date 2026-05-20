import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import worker, { __test__, type Env } from './worker';

const env: Env = {
  GITHUB_WEBHOOK_SECRET: 'incoming-secret',
  RELAY_FORWARD_SECRET: 'forward-secret',
  RELAY_DESTINATION_URL: 'https://desktop.local/webhook'
};

function requestWithSignature(body: string, signature: string): Request {
  return new Request('https://relay.example/webhook', {
    method: 'POST',
    headers: {
      'x-hub-signature-256': signature,
      'x-github-event': 'pull_request',
      'x-github-delivery': 'delivery-123',
      'content-type': 'application/json'
    },
    body
  });
}

describe('relay worker', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns 200 and forwards signed payload on valid signature', async () => {
    const body = JSON.stringify({ action: 'opened' });
    const signature = `sha256=${await __test__.sign(env.GITHUB_WEBHOOK_SECRET, body)}`;

    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response('ok', { status: 200 }));

    const response = await worker.fetch(requestWithSignature(body, signature), env);
    expect(response.status).toBe(200);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(env.RELAY_DESTINATION_URL);
    expect(init.method).toBe('POST');
    expect(init.body).toBe(body);

    const headers = init.headers as Headers;
    expect(headers.get('x-github-event')).toBe('pull_request');
    expect(headers.get('x-github-delivery')).toBe('delivery-123');
    expect(headers.get('x-relay-forwarded-by')).toBe('pr-cockpit-relay');
    expect(headers.get('x-relay-nonce')).toBeTruthy();
    expect(headers.get('x-relay-timestamp')).toBeTruthy();
    expect(headers.get('x-relay-signature-256')?.startsWith('sha256=')).toBe(true);

    const expectedRelaySignature = await __test__.sign(
      env.RELAY_FORWARD_SECRET,
      `${body}.${headers.get('x-relay-nonce')}.${headers.get('x-relay-timestamp')}`
    );
    expect(headers.get('x-relay-signature-256')).toBe(`sha256=${expectedRelaySignature}`);
  });

  it('returns 401 and does not forward on invalid signature', async () => {
    const body = JSON.stringify({ action: 'edited' });
    const fetchMock = vi.spyOn(globalThis, 'fetch');

    const response = await worker.fetch(
      requestWithSignature(body, 'sha256=deadbeefdeadbeefdeadbeefdeadbeef'),
      env
    );
    expect(response.status).toBe(401);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('returns 401 when signature header is missing', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch');
    const response = await worker.fetch(
      new Request('https://relay.example/webhook', {
        method: 'POST',
        body: JSON.stringify({ action: 'opened' })
      }),
      env
    );
    expect(response.status).toBe(401);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('returns 502 after one retry when destination returns 500', async () => {
    const body = JSON.stringify({ action: 'synchronize' });
    const signature = `sha256=${await __test__.sign(env.GITHUB_WEBHOOK_SECRET, body)}`;

    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response('down', { status: 500 }));

    const response = await worker.fetch(requestWithSignature(body, signature), env);

    expect(response.status).toBe(502);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('returns 502 after timeout retry path', async () => {
    const body = JSON.stringify({ action: 'closed' });
    const signature = `sha256=${await __test__.sign(env.GITHUB_WEBHOOK_SECRET, body)}`;

    const abortError = Object.assign(new Error('timed out'), { name: 'AbortError' });
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockRejectedValue(abortError);

    const response = await worker.fetch(requestWithSignature(body, signature), env);

    expect(response.status).toBe(502);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('rejects near-miss signatures using timing-safe comparison', async () => {
    const body = JSON.stringify({ action: 'reopened' });
    const valid = `sha256=${await __test__.sign(env.GITHUB_WEBHOOK_SECRET, body)}`;
    const nearMiss = valid.slice(0, -1) + (valid.endsWith('0') ? '1' : '0');

    expect(__test__.timingSafeEqualText(valid, nearMiss)).toBe(false);
    const fetchMock = vi.spyOn(globalThis, 'fetch');
    const response = await worker.fetch(requestWithSignature(body, nearMiss), env);
    expect(response.status).toBe(401);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
