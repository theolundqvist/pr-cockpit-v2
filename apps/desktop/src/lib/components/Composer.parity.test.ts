import { readFileSync } from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';

const { renderPreviewMock } = vi.hoisted(() => ({
  renderPreviewMock: vi.fn(async (body: string, repo: string | null) => ({
    html: `<p>${body.replaceAll('<', '&lt;').replaceAll('>', '&gt;')}</p><span data-repo="${repo ?? ''}"></span>`,
    cache_hit: false,
    content_hash: 'mock',
    cache_key: 'mock',
    renderer_version: 'mock'
  }))
}));

vi.mock('$lib/ipc/client', async () => {
  const actual = await vi.importActual<object>('$lib/ipc/client');
  return {
    ...actual,
    renderPreview: renderPreviewMock,
    listDrafts: vi.fn(async () => []),
    saveDraft: vi.fn(
      async (input: {
        id?: string;
        account_id: string;
        target_type: string;
        target_id: string;
        body: string;
      }) => ({
        id: input.id ?? 'draft-mock',
        account_id: input.account_id,
        target_type: input.target_type,
        target_id: input.target_id,
        body: input.body,
        created_at: 1,
        updated_at: 1
      })
    ),
    deleteDraft: vi.fn(async () => {}),
    submitMutation: vi.fn(async () => ({
      mutation_id: 'm-1',
      deduped: false,
      requires_confirmation: false,
      optimism_level: 'full',
      projected_changes: ['add_comment']
    }))
  };
});

type CorpusEntry = { body: string; repo: string };

function loadCorpusSubset(): CorpusEntry[] {
  const corpusPath = path.resolve(process.cwd(), '../../tools/markdown-corpus/corpus.json');
  const contents = readFileSync(corpusPath, 'utf8');
  const parsed = JSON.parse(contents) as Array<{ body: string; repo: string }>;
  const subset = parsed.slice(0, 20).map((entry) => ({ body: entry.body, repo: entry.repo }));
  subset.push({
    repo: 'fixture-org/repo-1',
    body: '```suggestion\nconst answer = 42;\n```'
  });
  return subset;
}

describe('Composer preview parity', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('matches timeline rendering output for 20 corpus entries', async () => {
    const { renderTimelineBody } = await import('$lib/data/pr-detail');
    const { renderComposerPreview } = await import('$lib/components/composer-model');
    const subset = loadCorpusSubset();
    for (const entry of subset) {
      const timelineHtml = await renderTimelineBody(entry.body, entry.repo);
      const composerHtml = await renderComposerPreview(entry.body, entry.repo);
      expect(composerHtml).toBe(timelineHtml);
    }
  });
});
