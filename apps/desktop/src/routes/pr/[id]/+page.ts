import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

import { getPrDetailBundle } from '$lib/data/pr-detail';
import { getActiveAccountId } from '$lib/state/cockpit';

export const prerender = false;

export const load: PageLoad = async ({ params }) => {
  const activeAccountId = getActiveAccountId();
  if (!activeAccountId) {
    throw error(400, 'No active account selected');
  }
  const bundle = await getPrDetailBundle(activeAccountId, params.id);
  if (!bundle) {
    throw error(404, 'Pull request not found');
  }
  return {
    activeAccountId,
    prId: params.id,
    bundle
  };
};
