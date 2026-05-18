export const KEY_SEQUENCE_TIMEOUT_MS = 500;

export type ComboBinding = {
  type: 'combo';
  commandId: string;
  key: string;
  ctrl?: boolean;
  meta?: boolean;
  alt?: boolean;
  shift?: boolean;
  mod?: boolean;
  allowInInput?: boolean;
  scope?: 'global' | 'palette';
  shortcutLabel: string;
};

export type SequenceBinding = {
  type: 'sequence';
  commandId: string;
  leadKey: string;
  key: string;
  shift?: boolean;
  allowInInput?: boolean;
  shortcutLabel: string;
};

export type KeyBinding = ComboBinding | SequenceBinding;

type SequenceState = {
  leadKey: string;
  startedAt: number;
};

export const KEY_BINDINGS: KeyBinding[] = [
  {
    type: 'combo',
    commandId: 'palette.open',
    key: 'k',
    mod: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+K / Cmd+K'
  },
  { type: 'sequence', commandId: 'inbox.open', leadKey: 'g', key: 'i', shortcutLabel: 'g i' },
  { type: 'sequence', commandId: 'pr.openByNumber', leadKey: 'g', key: 'p', shortcutLabel: 'g p' },
  {
    type: 'combo',
    commandId: 'account.switch',
    key: 'a',
    ctrl: true,
    shift: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Shift+A'
  },
  { type: 'combo', commandId: 'composer.open', key: 'c', shortcutLabel: 'c' },
  { type: 'combo', commandId: 'composer.focus', key: 'r', shortcutLabel: 'r' },
  { type: 'combo', commandId: 'thread.focusPrevious', key: 'k', shortcutLabel: 'k' },
  { type: 'combo', commandId: 'thread.focusNext', key: 'j', shortcutLabel: 'j' },
  { type: 'sequence', commandId: 'suggestion.apply', leadKey: 'g', key: 's', shortcutLabel: 'g s' },
  {
    type: 'sequence',
    commandId: 'suggestion.applyBatch',
    leadKey: 'g',
    key: 's',
    shift: true,
    shortcutLabel: 'g S'
  },
  {
    type: 'combo',
    commandId: 'thread.resolve',
    key: 'r',
    ctrl: true,
    shift: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Shift+R'
  },
  {
    type: 'combo',
    commandId: 'thread.unresolve',
    key: 'u',
    ctrl: true,
    shift: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Shift+U'
  },
  { type: 'combo', commandId: 'file.markViewed', key: 'v', shortcutLabel: 'v' },
  { type: 'combo', commandId: 'file.unmarkViewed', key: 'v', shift: true, shortcutLabel: 'V' },
  {
    type: 'combo',
    commandId: 'pr.openInGithub',
    key: 'o',
    ctrl: true,
    shift: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Shift+O'
  },
  {
    type: 'sequence',
    commandId: 'inbox.openInGithub',
    leadKey: 'g',
    key: 'g',
    shift: true,
    shortcutLabel: 'g G'
  },
  {
    type: 'combo',
    commandId: 'pr.refresh',
    key: 'r',
    mod: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+R / Cmd+R'
  },
  { type: 'combo', commandId: 'pr.refresh', key: 'F5', allowInInput: true, shortcutLabel: 'F5' },
  {
    type: 'combo',
    commandId: 'checks.rerunRun',
    key: 'y',
    ctrl: true,
    shift: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Shift+Y'
  },
  {
    type: 'combo',
    commandId: 'checks.rerunSuite',
    key: 'y',
    ctrl: true,
    alt: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Alt+Y'
  },
  {
    type: 'combo',
    commandId: 'settings.open',
    key: ',',
    ctrl: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+,'
  },
  {
    type: 'combo',
    commandId: 'composer.identity.switch',
    key: 'i',
    ctrl: true,
    shift: true,
    allowInInput: true,
    shortcutLabel: 'Ctrl+Shift+I'
  },
  {
    type: 'combo',
    commandId: 'palette.navigate.up',
    key: 'ArrowUp',
    scope: 'palette',
    shortcutLabel: 'ArrowUp'
  },
  {
    type: 'combo',
    commandId: 'palette.navigate.down',
    key: 'ArrowDown',
    scope: 'palette',
    shortcutLabel: 'ArrowDown'
  },
  {
    type: 'combo',
    commandId: 'palette.confirm',
    key: 'Enter',
    scope: 'palette',
    shortcutLabel: 'Enter'
  },
  {
    type: 'combo',
    commandId: 'palette.dismiss',
    key: 'Escape',
    scope: 'palette',
    shortcutLabel: 'Escape'
  }
];

function normalizeKey(key: string): string {
  return key.length === 1 ? key.toLowerCase() : key;
}

function hasOnlyShiftModifier(event: KeyboardEvent): boolean {
  return !event.ctrlKey && !event.metaKey && !event.altKey;
}

function matchesCombo(event: KeyboardEvent, binding: ComboBinding): boolean {
  const normalizedEventKey = normalizeKey(event.key);
  const normalizedBindingKey = normalizeKey(binding.key);
  if (normalizedEventKey !== normalizedBindingKey) {
    return false;
  }

  if (binding.mod) {
    if (!(event.ctrlKey || event.metaKey)) {
      return false;
    }
  } else {
    if (Boolean(binding.ctrl) !== event.ctrlKey) {
      return false;
    }
    if (Boolean(binding.meta) !== event.metaKey) {
      return false;
    }
  }

  if (Boolean(binding.alt) !== event.altKey) {
    return false;
  }
  if (Boolean(binding.shift) !== event.shiftKey) {
    return false;
  }
  return true;
}

function isInputTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) {
    return false;
  }
  if (element.isContentEditable) {
    return true;
  }
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(element.tagName);
}

export function shortcutsForCommand(commandId: string): string[] {
  return KEY_BINDINGS.filter((binding) => binding.commandId === commandId).map(
    (binding) => binding.shortcutLabel
  );
}

export function createKeymapResolver() {
  let pendingSequence: SequenceState | null = null;

  function resetSequence(): void {
    pendingSequence = null;
  }

  function resolve(
    event: KeyboardEvent,
    paletteOpen: boolean
  ): { commandId: string | null; consume: boolean } {
    const inputFocused = isInputTarget(event.target);
    const now = Date.now();
    const normalizedKey = normalizeKey(event.key);

    if (pendingSequence && now - pendingSequence.startedAt > KEY_SEQUENCE_TIMEOUT_MS) {
      pendingSequence = null;
    }

    if (paletteOpen) {
      const paletteBinding = KEY_BINDINGS.find(
        (binding) =>
          binding.type === 'combo' && binding.scope === 'palette' && matchesCombo(event, binding)
      ) as ComboBinding | undefined;
      if (paletteBinding) {
        return { commandId: paletteBinding.commandId, consume: true };
      }
    }

    const sequenceBindings = KEY_BINDINGS.filter(
      (binding): binding is SequenceBinding => binding.type === 'sequence'
    );
    if (pendingSequence && hasOnlyShiftModifier(event)) {
      const completed = sequenceBindings.find(
        (binding) =>
          binding.leadKey === pendingSequence?.leadKey &&
          normalizeKey(binding.key) === normalizedKey &&
          Boolean(binding.shift) === event.shiftKey
      );
      if (completed) {
        pendingSequence = null;
        if (!inputFocused || completed.allowInInput) {
          return { commandId: completed.commandId, consume: true };
        }
        return { commandId: null, consume: false };
      }
      pendingSequence = null;
    }

    if (!event.ctrlKey && !event.metaKey && !event.altKey) {
      const leadBinding = sequenceBindings.find(
        (binding) =>
          normalizeKey(binding.leadKey) === normalizedKey && (!inputFocused || binding.allowInInput)
      );
      if (leadBinding) {
        pendingSequence = { leadKey: normalizeKey(leadBinding.leadKey), startedAt: now };
        return { commandId: null, consume: true };
      }
    }

    const comboBindings = KEY_BINDINGS.filter(
      (binding): binding is ComboBinding =>
        binding.type === 'combo' && (binding.scope ?? 'global') === 'global'
    );
    for (const binding of comboBindings) {
      if (!matchesCombo(event, binding)) {
        continue;
      }
      if (inputFocused && !binding.allowInInput) {
        return { commandId: null, consume: false };
      }
      pendingSequence = null;
      return { commandId: binding.commandId, consume: true };
    }

    if (!event.ctrlKey && !event.metaKey && !event.altKey) {
      pendingSequence = null;
    }
    return { commandId: null, consume: false };
  }

  return {
    resolve,
    resetSequence
  };
}
