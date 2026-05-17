const extensionToLanguage: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  go: 'go',
  py: 'python',
  md: 'markdown'
};

export function languageFromPath(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase();
  if (!extension) {
    return 'text';
  }
  return extensionToLanguage[extension] ?? 'text';
}
