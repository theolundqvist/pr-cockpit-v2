export const ssr = false;
export const prerender = true;

import type { LayoutLoad } from './$types';
import { getInitInbox } from '$lib/ipc/client';

export const load: LayoutLoad = async () => {
  const boot = await getInitInbox();
  return { boot };
};
