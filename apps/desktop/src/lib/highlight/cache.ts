import type { HighlightLineTokens } from '$lib/highlight/types';

const DB_NAME = 'pr-cockpit-highlight-cache';
const STORE_NAME = 'tokens';
const DB_VERSION = 1;

type CacheRecord = {
  key: string;
  language: string;
  contentHash: string;
  lines: HighlightLineTokens[];
  createdAt: number;
};

function cacheKey(language: string, contentHash: string): string {
  return `${language}:${contentHash}`;
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onerror = () => reject(request.error);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: 'key' });
      }
    };
    request.onsuccess = () => resolve(request.result);
  });
}

export async function readTokenCache(
  language: string,
  contentHash: string
): Promise<HighlightLineTokens[] | null> {
  if (typeof indexedDB === 'undefined') {
    return null;
  }
  const db = await openDb();
  const key = cacheKey(language, contentHash);
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readonly');
    const request = tx.objectStore(STORE_NAME).get(key);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const result = request.result as CacheRecord | undefined;
      resolve(result?.lines ?? null);
    };
  });
}

export async function writeTokenCache(
  language: string,
  contentHash: string,
  lines: HighlightLineTokens[]
): Promise<void> {
  if (typeof indexedDB === 'undefined') {
    return;
  }
  const db = await openDb();
  const record: CacheRecord = {
    key: cacheKey(language, contentHash),
    language,
    contentHash,
    lines,
    createdAt: Date.now()
  };
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, 'readwrite');
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.objectStore(STORE_NAME).put(record);
  });
}
