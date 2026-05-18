import { get, writable } from 'svelte/store';

type ComposerTarget = {
  id: string;
  insertText: (text: string) => void;
};

const targets = new Map<string, ComposerTarget>();
export const focusedComposerIdStore = writable<string | null>(null);

export function registerComposerFocusTarget(target: ComposerTarget): () => void {
  targets.set(target.id, target);
  return () => {
    targets.delete(target.id);
    if (get(focusedComposerIdStore) === target.id) {
      focusedComposerIdStore.set(null);
    }
  };
}

export function setFocusedComposerId(id: string | null): void {
  focusedComposerIdStore.set(id);
}

export function insertIntoFocusedComposer(text: string): boolean {
  const id = get(focusedComposerIdStore);
  if (!id) {
    return false;
  }
  const target = targets.get(id);
  if (!target) {
    return false;
  }
  target.insertText(text);
  return true;
}
