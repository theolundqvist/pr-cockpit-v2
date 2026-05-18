import type {
  CheckAnnotationView,
  CheckRunSummary,
  PrCheckSummary,
  PrDetailSummary,
  PrFile,
  PrMetadataResponse,
  ReviewThread,
  SuggestionBlock,
  TimelineItem
} from '$lib/ipc/bindings';
import {
  getCheckSummary,
  getCheckAnnotations,
  getPrFiles,
  getPrMetadata,
  getPrPatch,
  getPrSummary,
  getPrTimeline,
  getReviewThreads,
  getSuggestionBlocks,
  renderPreview
} from '$lib/ipc/client';

export type RenderedTimelineItem = TimelineItem & {
  rendered_html: string;
  renderer_version: string;
};

export type PrDetailBundle = {
  summary: PrDetailSummary;
  metadata: PrMetadataResponse;
  timeline: RenderedTimelineItem[];
  review_threads: ReviewThread[];
  suggestion_blocks: SuggestionBlock[];
  checks: PrCheckSummary;
  check_annotations: CheckAnnotationView[];
  files: PrFile[];
  patch: string;
  patch_blob_sha: string | null;
};

const detailCache = new Map<string, Promise<PrDetailBundle | null>>();

function key(accountId: string, prId: string): string {
  return `${accountId}:${prId}`;
}

async function hydrateTimeline(timeline: TimelineItem[]): Promise<RenderedTimelineItem[]> {
  return Promise.all(
    timeline.map(async (item) => {
      const rendered = await renderPreview(item.body, null);
      return {
        ...item,
        rendered_html: rendered.html,
        renderer_version: rendered.renderer_version
      };
    })
  );
}

export async function renderTimelineBody(body: string, repo: string | null): Promise<string> {
  const rendered = await renderPreview(body, repo);
  return rendered.html;
}

async function loadBundle(accountId: string, prId: string): Promise<PrDetailBundle | null> {
  const summary = await getPrSummary(accountId, prId);
  if (!summary) {
    return null;
  }
  const [
    metadata,
    timelinePage,
    threadsPage,
    suggestionBlocks,
    checks,
    checkAnnotations,
    filesResponse,
    patchResponse
  ] = await Promise.all([
    getPrMetadata(accountId, prId),
    getPrTimeline(accountId, prId),
    getReviewThreads(accountId, prId),
    getSuggestionBlocks(accountId, prId),
    getCheckSummary(accountId, prId),
    getCheckAnnotations(accountId, prId),
    getPrFiles(accountId, prId, summary.head_sha),
    getPrPatch(accountId, prId, summary.head_sha)
  ]);
  const timeline = await hydrateTimeline(timelinePage.items);
  return {
    summary,
    metadata,
    timeline,
    review_threads: threadsPage.threads,
    suggestion_blocks: suggestionBlocks,
    checks: normalizeChecks(checks),
    check_annotations: checkAnnotations,
    files: filesResponse.files,
    patch: patchResponse.patch ?? '',
    patch_blob_sha: patchResponse.patch_blob_sha
  };
}

function normalizeChecks(checks: PrCheckSummary): PrCheckSummary {
  const runs = [...checks.runs].sort(
    (left, right) => (right.completed_at ?? 0) - (left.completed_at ?? 0)
  );
  return {
    ...checks,
    runs
  };
}

export function prDetailPreload(accountId: string, prId: string): Promise<PrDetailBundle | null> {
  const cacheKey = key(accountId, prId);
  if (!detailCache.has(cacheKey)) {
    detailCache.set(cacheKey, loadBundle(accountId, prId));
  }
  return detailCache.get(cacheKey)!;
}

export async function getPrDetailBundle(
  accountId: string,
  prId: string,
  forceRefresh = false
): Promise<PrDetailBundle | null> {
  const cacheKey = key(accountId, prId);
  if (forceRefresh) {
    detailCache.delete(cacheKey);
  }
  return prDetailPreload(accountId, prId);
}

export function invalidatePrDetail(accountId: string, prId: string): void {
  detailCache.delete(key(accountId, prId));
}

export function clearPrDetailCache(): void {
  detailCache.clear();
}

export function checksRollupBadge(checks: PrCheckSummary): {
  label: string;
  statusClass: string;
  runs: CheckRunSummary[];
} {
  if (checks.total_runs === 0) {
    return { label: 'No checks', statusClass: 'color-fg-muted', runs: [] };
  }
  if (checks.failed_runs > 0) {
    return {
      label: `${checks.failed_runs} failing`,
      statusClass: 'color-fg-danger',
      runs: checks.runs
    };
  }
  if (checks.pending_runs > 0) {
    return {
      label: `${checks.pending_runs} pending`,
      statusClass: 'color-fg-attention',
      runs: checks.runs
    };
  }
  return {
    label: `${checks.successful_runs} passing`,
    statusClass: 'color-fg-success',
    runs: checks.runs
  };
}
