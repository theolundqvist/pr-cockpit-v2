declare global {
  namespace App {}

  interface Window {
    __INBOX_SEED__?: import('$lib/ipc/bindings').InitInboxResponse | null;
  }

  type EmscriptenModule = {
    locateFile?: (path: string, prefix: string) => string;
  };
}

export {};
