export function formatRelative(epochSeconds: number | null): string {
  if (!epochSeconds) {
    return 'just now';
  }
  const now = Math.floor(Date.now() / 1000);
  const delta = Math.max(0, now - epochSeconds);
  if (delta < 60) {
    return `${delta}s`;
  }
  if (delta < 3600) {
    return `${Math.floor(delta / 60)}m`;
  }
  if (delta < 86_400) {
    return `${Math.floor(delta / 3600)}h`;
  }
  if (delta < 604_800) {
    return `${Math.floor(delta / 86_400)}d`;
  }
  return `${Math.floor(delta / 604_800)}w`;
}
