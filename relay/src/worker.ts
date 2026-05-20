import { timingSafeEqual } from 'node:crypto';

export interface Env {
  GITHUB_WEBHOOK_SECRET: string;
  RELAY_FORWARD_SECRET: string;
  RELAY_DESTINATION_URL: string;
}

const GITHUB_SIGNATURE_HEADER = 'x-hub-signature-256';
const GITHUB_EVENT_HEADER = 'x-github-event';
const GITHUB_DELIVERY_HEADER = 'x-github-delivery';
const RELAY_SIGNATURE_HEADER = 'x-relay-signature-256';
const RELAY_NONCE_HEADER = 'x-relay-nonce';
const RELAY_TIMESTAMP_HEADER = 'x-relay-timestamp';
const RELAY_FORWARDED_BY_HEADER = 'x-relay-forwarded-by';

function encode(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

async function sign(secret: string, payload: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    'raw',
    encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign']
  );
  const digest = await crypto.subtle.sign('HMAC', key, encode(payload));
  return toHex(new Uint8Array(digest));
}

function timingSafeEqualText(left: string, right: string): boolean {
  const leftBuffer = Buffer.from(left);
  const rightBuffer = Buffer.from(right);
  const maxLen = Math.max(leftBuffer.length, rightBuffer.length, 1);

  const paddedLeft = Buffer.alloc(maxLen);
  const paddedRight = Buffer.alloc(maxLen);
  leftBuffer.copy(paddedLeft);
  rightBuffer.copy(paddedRight);

  return timingSafeEqual(paddedLeft, paddedRight) && leftBuffer.length === rightBuffer.length;
}

function randomNonce(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return toHex(bytes);
}

async function verifyGithubSignature(
  bodyText: string,
  receivedHeader: string | null,
  secret: string
): Promise<boolean> {
  if (!receivedHeader) {
    return false;
  }
  const expected = `sha256=${await sign(secret, bodyText)}`;
  return timingSafeEqualText(expected, receivedHeader.trim());
}

function buildForwardHeaders(
  request: Request,
  relaySignature: string,
  nonce: string,
  timestamp: string
): Headers {
  const headers = new Headers();
  const contentType = request.headers.get('content-type');
  if (contentType) {
    headers.set('content-type', contentType);
  }

  const githubEvent = request.headers.get(GITHUB_EVENT_HEADER);
  if (githubEvent) {
    headers.set('x-github-event', githubEvent);
  }
  const githubDelivery = request.headers.get(GITHUB_DELIVERY_HEADER);
  if (githubDelivery) {
    headers.set('x-github-delivery', githubDelivery);
  }

  headers.set(RELAY_SIGNATURE_HEADER, `sha256=${relaySignature}`);
  headers.set(RELAY_NONCE_HEADER, nonce);
  headers.set(RELAY_TIMESTAMP_HEADER, timestamp);
  headers.set(RELAY_FORWARDED_BY_HEADER, 'pr-cockpit-relay');
  return headers;
}

async function forwardOnce(env: Env, bodyText: string, headers: Headers): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 5_000);
  try {
    return await fetch(env.RELAY_DESTINATION_URL, {
      method: 'POST',
      headers,
      body: bodyText,
      signal: controller.signal
    });
  } finally {
    clearTimeout(timeoutId);
  }
}

function shouldRetry(error: unknown, response: Response | null): boolean {
  if (response && response.status >= 500) {
    return true;
  }
  if (!error) {
    return false;
  }
  if (error instanceof Error) {
    return error.name === 'AbortError';
  }
  return false;
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method !== 'POST') {
      return new Response('Method Not Allowed', { status: 405 });
    }

    const bodyText = await request.text();
    const receivedSignature = request.headers.get(GITHUB_SIGNATURE_HEADER);
    const valid = await verifyGithubSignature(bodyText, receivedSignature, env.GITHUB_WEBHOOK_SECRET);
    if (!valid) {
      return new Response('Invalid signature', { status: 401 });
    }

    const timestamp = Math.floor(Date.now() / 1000).toString();
    const nonce = randomNonce();
    const signedPayload = `${bodyText}.${nonce}.${timestamp}`;
    const relaySignature = await sign(env.RELAY_FORWARD_SECRET, signedPayload);
    const headers = buildForwardHeaders(request, relaySignature, nonce, timestamp);

    let firstError: unknown = null;
    let firstResponse: Response | null = null;
    try {
      firstResponse = await forwardOnce(env, bodyText, headers);
    } catch (error) {
      firstError = error;
    }

    if (!shouldRetry(firstError, firstResponse)) {
      if (firstResponse && firstResponse.ok) {
        return new Response('ok', { status: 200 });
      }
      if (firstResponse && firstResponse.status < 500) {
        return new Response('ok', { status: 200 });
      }
      return new Response('Forwarding failed', { status: 502 });
    }

    await new Promise((resolve) => setTimeout(resolve, 1_000));

    try {
      const secondResponse = await forwardOnce(env, bodyText, headers);
      if (secondResponse.ok || secondResponse.status < 500) {
        return new Response('ok', { status: 200 });
      }
    } catch (_error) {
      // fall through to terminal gateway error
    }

    return new Response('Forwarding failed after retry', { status: 502 });
  }
};

export const __test__ = {
  timingSafeEqualText,
  sign
};
