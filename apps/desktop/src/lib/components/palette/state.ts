import { writable } from 'svelte/store';

export type PaletteMode = 'commands' | 'account-switch' | 'pr-open-number';

export type PaletteState = {
  open: boolean;
  mode: PaletteMode;
  props?: Record<string, unknown>;
  initialQuery?: string;
};

type PaletteController = {
  navigateUp: () => void;
  navigateDown: () => void;
  confirm: () => void;
  dismiss: () => void;
};

const initialState: PaletteState = {
  open: false,
  mode: 'commands',
  props: {},
  initialQuery: ''
};

let controller: PaletteController | null = null;

export const commandPaletteState = writable<PaletteState>(initialState);

export function openCommandPalette(initialQuery = ''): void {
  commandPaletteState.set({
    open: true,
    mode: 'commands',
    initialQuery,
    props: {}
  });
}

export function openAccountSwitchPalette(): void {
  commandPaletteState.set({
    open: true,
    mode: 'account-switch',
    props: {},
    initialQuery: ''
  });
}

export function openPrNumberPalette(): void {
  commandPaletteState.set({
    open: true,
    mode: 'pr-open-number',
    props: {},
    initialQuery: ''
  });
}

export function closeCommandPalette(): void {
  commandPaletteState.set(initialState);
}

export function setPaletteController(nextController: PaletteController | null): void {
  controller = nextController;
}

export function paletteNavigateUp(): void {
  controller?.navigateUp();
}

export function paletteNavigateDown(): void {
  controller?.navigateDown();
}

export function paletteConfirm(): void {
  controller?.confirm();
}

export function paletteDismiss(): void {
  controller?.dismiss();
}
