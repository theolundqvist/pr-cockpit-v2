import type { InboxItem, WorktreeView } from '$lib/ipc/bindings';

export function syntheticLabels(row: InboxItem): string[] {
  if (row.pr_id === 'pr_1') {
    return ['needs-review'];
  }
  if (row.draft) {
    return ['draft'];
  }
  if (row.unread_notification_count > 2) {
    return ['attention'];
  }
  return [];
}

export function checkLabel(row: InboxItem): string {
  if (row.mergeable_state === 'clean') {
    return 'Checks green';
  }
  if (row.merge_state_status) {
    return row.merge_state_status;
  }
  return 'Pending';
}

export function mappedWorktreeForPr(prId: string, worktrees: WorktreeView[]): WorktreeView | null {
  const candidates = worktrees.filter((worktree) => worktree.mapped_pr_id === prId);
  if (candidates.length === 0) {
    return null;
  }
  candidates.sort(
    (left, right) => (right.mapping_confidence ?? 0) - (left.mapping_confidence ?? 0)
  );
  return candidates[0] ?? null;
}
