import type { MutationKind, SubmittedMutation } from '$lib/ipc/bindings';
import { renderPreview, submitMutation, submitReviewComment } from '$lib/ipc/client';

export async function renderComposerPreview(body: string, repo: string | null): Promise<string> {
  const rendered = await renderPreview(body, repo);
  return rendered.html;
}

export async function submitComposerMutation(
  accountId: string,
  kind: MutationKind,
  payload: Record<string, unknown>
): Promise<{ submission: SubmittedMutation; elapsedMs: number }> {
  const started = performance.now();
  const submission =
    kind === 'add_review_comment'
      ? await submitReviewComment(accountId, payload)
      : await submitMutation(accountId, kind, JSON.stringify(payload));
  return { submission, elapsedMs: performance.now() - started };
}

export function insertSuggestionBlock(
  currentBody: string,
  selectedLines: string[],
  selectionStart: number,
  selectionEnd: number
): { nextBody: string; nextCaret: number } {
  const seed = selectedLines.length > 0 ? selectedLines.join('\n') : '';
  const block = `\`\`\`suggestion\n${seed}\n\`\`\``;
  const prefix = currentBody.slice(0, selectionStart);
  const suffix = currentBody.slice(selectionEnd);
  const needsLeadingBreak = prefix.length > 0 && !prefix.endsWith('\n');
  const needsTrailingBreak = suffix.length > 0 && !suffix.startsWith('\n');
  const insertion = `${needsLeadingBreak ? '\n\n' : ''}${block}${needsTrailingBreak ? '\n\n' : ''}`;
  return insertTextAtSelection(currentBody, insertion, selectionStart, selectionEnd);
}

export function insertTextAtSelection(
  currentBody: string,
  text: string,
  selectionStart: number,
  selectionEnd: number
): { nextBody: string; nextCaret: number } {
  const prefix = currentBody.slice(0, selectionStart);
  const suffix = currentBody.slice(selectionEnd);
  const nextBody = `${prefix}${text}${suffix}`;
  return { nextBody, nextCaret: prefix.length + text.length };
}
