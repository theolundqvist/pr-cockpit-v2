export interface Command {
  id: string;
  title: string;
  section: string;
  keywords?: string[];
  shortcut?: string;
  visibility?: (ctx: CommandContext) => boolean;
  run: (ctx: CommandContext) => Promise<void> | void;
}

export interface CommandContext {
  activeAccountId: string;
  activeRoute: string;
  activePrId?: string | undefined;
  activeFilePath?: string | undefined;
  activeSuggestionId?: string | undefined;
  openModal: (id: string, props?: unknown) => void;
  navigate: (path: string) => void;
  submitMutation: (kind: string, payload: unknown) => Promise<void>;
  [key: string]: unknown;
}

const RECENT_STORAGE_KEY = 'palette.recent';
const RECENT_LIMIT = 5;

const commandMap = new Map<string, Command>();
const dynamicCommandSources = new Map<string, string[]>();
let defaultOrder: string[] = [];

function sortCommands(a: Command, b: Command): number {
  const sectionOrder = a.section.localeCompare(b.section);
  if (sectionOrder !== 0) {
    return sectionOrder;
  }
  return a.title.localeCompare(b.title);
}

function recomputeDefaultOrder(): void {
  defaultOrder = [...commandMap.values()].sort(sortCommands).map((command) => command.id);
}

function rememberCommand(command: Command): void {
  commandMap.set(command.id, command);
}

function removeCommand(id: string): void {
  commandMap.delete(id);
}

function readRecentIds(): string[] {
  if (typeof window === 'undefined') {
    return [];
  }
  try {
    const raw = window.localStorage.getItem(RECENT_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter((value): value is string => typeof value === 'string');
  } catch {
    return [];
  }
}

function writeRecentIds(ids: string[]): void {
  if (typeof window === 'undefined') {
    return;
  }
  try {
    window.localStorage.setItem(RECENT_STORAGE_KEY, JSON.stringify(ids.slice(0, RECENT_LIMIT)));
  } catch {
    // Ignore storage failures in constrained contexts.
  }
}

function shouldRecordRecent(commandId: string): boolean {
  return !commandId.startsWith('palette.navigate') && commandId !== 'palette.confirm';
}

export function registerStaticCommands(commands: Command[]): void {
  for (const command of commands) {
    rememberCommand(command);
  }
  recomputeDefaultOrder();
}

export function setDynamicCommands(source: string, commands: Command[]): void {
  const previousIds = dynamicCommandSources.get(source) ?? [];
  for (const id of previousIds) {
    removeCommand(id);
  }
  for (const command of commands) {
    rememberCommand(command);
  }
  dynamicCommandSources.set(
    source,
    commands.map((command) => command.id)
  );
  recomputeDefaultOrder();
}

export function getCommandById(id: string): Command | undefined {
  return commandMap.get(id);
}

export function listCommands(ctx: CommandContext): Command[] {
  const listed: Command[] = [];
  for (const id of defaultOrder) {
    const command = commandMap.get(id);
    if (!command) {
      continue;
    }
    if (command.visibility && !command.visibility(ctx)) {
      continue;
    }
    listed.push(command);
  }
  return listed;
}

export function getRecentCommandIds(): string[] {
  return readRecentIds();
}

export function clearRecentCommandIds(): void {
  writeRecentIds([]);
}

export async function executeCommand(id: string, ctx: CommandContext): Promise<boolean> {
  const command = commandMap.get(id);
  if (!command) {
    return false;
  }
  if (command.visibility && !command.visibility(ctx)) {
    return false;
  }
  await command.run(ctx);
  if (shouldRecordRecent(command.id)) {
    const nextRecent = [command.id, ...readRecentIds().filter((entry) => entry !== command.id)];
    writeRecentIds(nextRecent.slice(0, RECENT_LIMIT));
  }
  return true;
}
