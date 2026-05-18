import type { SavedReply } from '$lib/ipc/bindings';
import {
  paletteConfirm,
  paletteDismiss,
  paletteNavigateDown,
  paletteNavigateUp
} from '$lib/components/palette/state';
import type { Command, CommandContext } from './registry';

type UiAction =
  | 'composer.open'
  | 'composer.focus'
  | 'composer.identity.switch'
  | 'thread.focusNext'
  | 'thread.focusPrevious'
  | 'suggestion.apply'
  | 'suggestion.applyBatch'
  | 'thread.resolve'
  | 'thread.unresolve'
  | 'file.markViewed'
  | 'file.unmarkViewed'
  | 'pr.refresh'
  | 'checks.rerunRun'
  | 'checks.rerunSuite'
  | 'theme.toggle';

function invokeAction(ctx: CommandContext, action: UiAction): Promise<void> {
  const invoke = ctx.invokeAction;
  if (typeof invoke !== 'function') {
    return Promise.resolve();
  }
  return Promise.resolve((invoke as (name: UiAction) => Promise<void> | void)(action));
}

function openExternalUrl(ctx: CommandContext, url: string | null | undefined): Promise<void> {
  if (!url) {
    return Promise.resolve();
  }
  const opener = ctx.openExternalUrl;
  if (typeof opener !== 'function') {
    return Promise.resolve();
  }
  return Promise.resolve((opener as (target: string) => Promise<void> | void)(url));
}

function hasSuggestionContext(ctx: CommandContext): boolean {
  return Boolean(ctx.activeSuggestionId || ctx.hasSuggestions || ctx.hasBatchSuggestions);
}

export const STATIC_COMMANDS: Command[] = [
  {
    id: 'palette.open',
    title: 'Open command palette',
    section: 'Command palette',
    keywords: ['command', 'search', 'shortcut'],
    shortcut: 'Ctrl+K / Cmd+K',
    run: (ctx) => ctx.openModal('palette.commands')
  },
  {
    id: 'inbox.open',
    title: 'Open inbox',
    section: 'Navigation',
    keywords: ['home', 'pull requests'],
    shortcut: 'g i',
    run: (ctx) => ctx.navigate('/')
  },
  {
    id: 'pr.openByNumber',
    title: 'Open PR by number',
    section: 'Navigation',
    keywords: ['pull request', 'goto'],
    shortcut: 'g p',
    run: (ctx) => ctx.openModal('pr.openByNumber')
  },
  {
    id: 'account.switch',
    title: 'Switch account',
    section: 'Navigation',
    keywords: ['account', 'identity'],
    shortcut: 'Ctrl+Shift+A',
    run: (ctx) => ctx.openModal('account.switch')
  },
  {
    id: 'settings.open',
    title: 'Open settings',
    section: 'Navigation',
    keywords: ['preferences', 'configuration'],
    shortcut: 'Ctrl+,',
    run: (ctx) => ctx.navigate('/settings/keyboard')
  },
  {
    id: 'inbox.openInGithub',
    title: 'Open inbox in github.com',
    section: 'Navigation',
    keywords: ['browser', 'issues'],
    shortcut: 'g G',
    run: async (ctx) => {
      const url = typeof ctx.inboxUrl === 'string' ? ctx.inboxUrl : null;
      await openExternalUrl(ctx, url);
    }
  },
  {
    id: 'pr.openInGithub',
    title: 'Open in github.com',
    section: 'Navigation',
    keywords: ['pull request', 'browser'],
    shortcut: 'Ctrl+Shift+O',
    visibility: (ctx) => Boolean(ctx.activePrId),
    run: async (ctx) => {
      const url = typeof ctx.activePrUrl === 'string' ? ctx.activePrUrl : null;
      await openExternalUrl(ctx, url);
    }
  },
  {
    id: 'composer.open',
    title: 'Compose comment',
    section: 'Composer',
    keywords: ['comment', 'reply'],
    shortcut: 'c',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'composer.open');
    }
  },
  {
    id: 'composer.focus',
    title: 'Focus composer',
    section: 'Composer',
    keywords: ['reply', 'textarea'],
    shortcut: 'r',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'composer.focus');
    }
  },
  {
    id: 'thread.focusPrevious',
    title: 'Focus previous thread',
    section: 'Review',
    keywords: ['thread', 'previous'],
    shortcut: 'k',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'thread.focusPrevious');
    }
  },
  {
    id: 'thread.focusNext',
    title: 'Focus next thread',
    section: 'Review',
    keywords: ['thread', 'next'],
    shortcut: 'j',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'thread.focusNext');
    }
  },
  {
    id: 'composer.identity.switch',
    title: 'Switch composer posting identity',
    section: 'Composer',
    keywords: ['posting identity', 'account'],
    shortcut: 'Ctrl+Shift+I',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'composer.identity.switch');
    }
  },
  {
    id: 'suggestion.apply',
    title: 'Apply suggestion',
    section: 'Review',
    keywords: ['suggestion', 'apply'],
    shortcut: 'g s',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/') && hasSuggestionContext(ctx),
    run: async (ctx) => {
      await invokeAction(ctx, 'suggestion.apply');
    }
  },
  {
    id: 'suggestion.applyBatch',
    title: 'Apply suggestions (batch)',
    section: 'Review',
    keywords: ['suggestion', 'batch', 'worktree'],
    shortcut: 'g S',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/') && Boolean(ctx.hasBatchSuggestions),
    run: async (ctx) => {
      await invokeAction(ctx, 'suggestion.applyBatch');
    }
  },
  {
    id: 'thread.resolve',
    title: 'Resolve thread',
    section: 'Review',
    keywords: ['thread', 'conversation'],
    shortcut: 'Ctrl+Shift+R',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'thread.resolve');
    }
  },
  {
    id: 'thread.unresolve',
    title: 'Unresolve thread',
    section: 'Review',
    keywords: ['thread', 'conversation'],
    shortcut: 'Ctrl+Shift+U',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'thread.unresolve');
    }
  },
  {
    id: 'file.markViewed',
    title: 'Mark file viewed',
    section: 'Diff',
    keywords: ['file', 'viewed'],
    shortcut: 'v',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'file.markViewed');
    }
  },
  {
    id: 'file.unmarkViewed',
    title: 'Unmark file viewed',
    section: 'Diff',
    keywords: ['file', 'viewed'],
    shortcut: 'V',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'file.unmarkViewed');
    }
  },
  {
    id: 'checks.rerunRun',
    title: 'Rerun check run',
    section: 'Checks',
    keywords: ['check run', 'rerun'],
    shortcut: 'Ctrl+Shift+Y',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/') && Boolean(ctx.activeCheckRunId),
    run: async (ctx) => {
      await invokeAction(ctx, 'checks.rerunRun');
    }
  },
  {
    id: 'checks.rerunSuite',
    title: 'Rerun check suite',
    section: 'Checks',
    keywords: ['check suite', 'rerun'],
    shortcut: 'Ctrl+Alt+Y',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/') && Boolean(ctx.activeCheckSuiteId),
    run: async (ctx) => {
      await invokeAction(ctx, 'checks.rerunSuite');
    }
  },
  {
    id: 'pr.refresh',
    title: 'Refresh pull request',
    section: 'Checks',
    keywords: ['reload', 'refresh'],
    shortcut: 'Ctrl+R / Cmd+R / F5',
    visibility: (ctx) => ctx.activeRoute.startsWith('/pr/'),
    run: async (ctx) => {
      await invokeAction(ctx, 'pr.refresh');
    }
  },
  {
    id: 'theme.toggle',
    title: 'Toggle theme',
    section: 'Appearance',
    keywords: ['theme', 'dark', 'light'],
    run: async (ctx) => {
      await invokeAction(ctx, 'theme.toggle');
    }
  },
  {
    id: 'palette.navigate.up',
    title: 'Palette: navigate up',
    section: 'Command palette',
    shortcut: 'ArrowUp',
    visibility: (ctx) => Boolean(ctx.paletteOpen),
    run: () => {
      paletteNavigateUp();
    }
  },
  {
    id: 'palette.navigate.down',
    title: 'Palette: navigate down',
    section: 'Command palette',
    shortcut: 'ArrowDown',
    visibility: (ctx) => Boolean(ctx.paletteOpen),
    run: () => {
      paletteNavigateDown();
    }
  },
  {
    id: 'palette.confirm',
    title: 'Palette: confirm',
    section: 'Command palette',
    shortcut: 'Enter',
    visibility: (ctx) => Boolean(ctx.paletteOpen),
    run: () => {
      paletteConfirm();
    }
  },
  {
    id: 'palette.dismiss',
    title: 'Palette: dismiss',
    section: 'Command palette',
    shortcut: 'Escape',
    visibility: (ctx) => Boolean(ctx.paletteOpen),
    run: () => {
      paletteDismiss();
    }
  }
];

export function buildSavedReplyCommands(replies: SavedReply[]): Command[] {
  return replies.map((reply) => ({
    id: `savedReply.insert.${reply.id}`,
    title: `Insert: ${reply.name}`,
    section: 'Saved replies',
    keywords: ['saved reply', reply.name],
    run: (ctx) => {
      const insert = ctx.insertIntoComposer;
      if (typeof insert === 'function') {
        const inserted = (insert as (text: string) => boolean)(reply.body);
        if (!inserted) {
          void invokeAction(ctx, 'composer.focus');
        }
      }
    }
  }));
}
