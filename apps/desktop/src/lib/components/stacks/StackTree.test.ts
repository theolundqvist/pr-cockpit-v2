import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import StackTree from '$lib/components/stacks/StackTree.svelte';
import { MOCK_STACKS_BY_SCOPE } from '$lib/mock/fixtures';

describe('StackTree', () => {
  it('renders linear stack rows when expanded', async () => {
    const stacks = MOCK_STACKS_BY_SCOPE['github.com:fixture-user:repo_2'] ?? [];
    render(StackTree, {
      props: {
        stacks,
        graphiteEnabled: false
      }
    });

    await fireEvent.click(screen.getByTestId('stack-summary-stack-linear-repo2'));
    expect(screen.getAllByTestId('stack-row').length).toBeGreaterThanOrEqual(3);
  });

  it('shows DAG warning banner for ambiguous stacks', async () => {
    const stacks = MOCK_STACKS_BY_SCOPE['github.com:fixture-user:repo_2'] ?? [];
    render(StackTree, {
      props: {
        stacks,
        graphiteEnabled: true
      }
    });

    await fireEvent.click(screen.getByTestId('stack-summary-stack-dag-repo2'));
    expect(screen.getByTestId('stack-dag-warning')).toBeTruthy();
  });
});
