import { writable } from 'svelte/store';

export const savedRepliesPaletteOpenStore = writable(false);

export function openSavedRepliesPalette(): void {
  savedRepliesPaletteOpenStore.set(true);
}

export function closeSavedRepliesPalette(): void {
  savedRepliesPaletteOpenStore.set(false);
}

export const savedRepliesPaletteCommand = {
  id: 'saved-replies.palette',
  title: 'Open saved replies palette',
  shortcut: 'Ctrl+Shift+.',
  run: openSavedRepliesPalette
};
