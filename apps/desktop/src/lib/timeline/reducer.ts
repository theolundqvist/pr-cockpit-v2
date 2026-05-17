import type { PrDetailSummary, PrMetadataResponse, ReviewThread } from '$lib/ipc/bindings';
import type { RenderedTimelineItem } from '$lib/data/pr-detail';

export type ConversationEntry =
  | {
      id: string;
      kind: 'comment' | 'review';
      title: string;
      author: string;
      createdAt: number;
      html: string;
      reviewState: string | null;
    }
  | {
      id: string;
      kind: 'thread';
      title: string;
      createdAt: number;
      path: string;
      line: number | null;
      side: string | null;
      isOutdated: boolean;
      commentCount: number;
    }
  | {
      id: string;
      kind: 'event';
      title: string;
      createdAt: number;
      details: string;
    };

function timelineEntryFromItem(item: RenderedTimelineItem): ConversationEntry {
  const isReview = item.item_kind === 'review';
  return {
    id: item.item_id,
    kind: isReview ? 'review' : 'comment',
    title: isReview ? 'Review' : 'Comment',
    author: item.author_login ?? 'ghost',
    createdAt: item.created_at,
    html: item.rendered_html,
    reviewState: item.review_state
  };
}

function threadEntries(threads: ReviewThread[]): ConversationEntry[] {
  return threads.map((thread) => ({
    id: thread.id,
    kind: 'thread',
    title: `Thread on ${thread.path}`,
    createdAt: thread.updated_at,
    path: thread.path,
    line: thread.line,
    side: thread.side,
    isOutdated: thread.is_outdated,
    commentCount: thread.comment_count
  }));
}

function metadataEvents(
  summary: PrDetailSummary,
  metadata: PrMetadataResponse
): ConversationEntry[] {
  const events: ConversationEntry[] = [
    {
      id: `event:state:${summary.pr_id}`,
      kind: 'event',
      title: 'State',
      createdAt: summary.updated_at,
      details: `${summary.state.toUpperCase()} (${summary.draft ? 'Draft' : 'Ready for review'})`
    }
  ];
  for (const label of metadata.labels) {
    events.push({
      id: `event:label:${label.label_name}`,
      kind: 'event',
      title: 'Label',
      createdAt: summary.updated_at,
      details: `Applied label ${label.label_name}`
    });
  }
  for (const assignee of metadata.assignees) {
    events.push({
      id: `event:assignee:${assignee.user_id}`,
      kind: 'event',
      title: 'Assignee',
      createdAt: summary.updated_at,
      details: `${assignee.login ?? assignee.user_id} assigned`
    });
  }
  for (const reviewer of metadata.requested_reviewers) {
    events.push({
      id: `event:reviewer:${reviewer.user_id}`,
      kind: 'event',
      title: 'Reviewer',
      createdAt: reviewer.requested_at,
      details: `Requested ${reviewer.login ?? reviewer.user_id}`
    });
  }
  return events;
}

export function reduceConversationTimeline(
  summary: PrDetailSummary,
  timelineItems: RenderedTimelineItem[],
  reviewThreads: ReviewThread[],
  metadata: PrMetadataResponse
): ConversationEntry[] {
  const entries = [
    ...timelineItems.map(timelineEntryFromItem),
    ...threadEntries(reviewThreads),
    ...metadataEvents(summary, metadata)
  ];
  return entries.sort((left, right) => right.createdAt - left.createdAt);
}
