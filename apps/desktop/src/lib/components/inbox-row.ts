import type { InboxItem } from '$lib/ipc/bindings';

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
