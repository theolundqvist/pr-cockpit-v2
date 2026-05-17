import type { MutationKind, SubmittedMutation } from '$lib/ipc/bindings';
import { renderPreview, submitMutation } from '$lib/ipc/client';

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
  const submission = await submitMutation(accountId, kind, JSON.stringify(payload));
  return { submission, elapsedMs: performance.now() - started };
}
