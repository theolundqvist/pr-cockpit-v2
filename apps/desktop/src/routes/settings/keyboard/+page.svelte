<script lang="ts">
  import { STATIC_COMMANDS } from '$lib/commands/commands';
  import { shortcutsForCommand } from '$lib/commands/keymap';

  const grouped = new Map<string, Array<{ title: string; shortcuts: string[] }>>();
  for (const command of STATIC_COMMANDS) {
    if (command.id.startsWith('palette.navigate') || command.id === 'palette.confirm' || command.id === 'palette.dismiss') {
      continue;
    }
    const shortcuts = shortcutsForCommand(command.id);
    if (shortcuts.length === 0) {
      continue;
    }
    const section = grouped.get(command.section) ?? [];
    section.push({ title: command.title, shortcuts });
    grouped.set(command.section, section);
  }
  const sections = [...grouped.entries()].map(([name, commands]) => ({
    name,
    commands: commands.sort((left, right) => left.title.localeCompare(right.title))
  }));
</script>

<main class="px-3 py-3">
  <section class="Box">
    <div class="Box-header d-flex flex-items-center flex-justify-between">
      <h1 class="f3 m-0">Keyboard shortcuts</h1>
      <a class="btn btn-sm" href="/settings">Back to settings</a>
    </div>
    <div class="Box-body">
      {#each sections as section}
        <div class="mb-3">
          <h2 class="f5 mb-2">{section.name}</h2>
          <ul class="list-style-none m-0 p-0">
            {#each section.commands as command}
              <li class="d-flex flex-items-center flex-justify-between py-1 border-bottom color-border-muted">
                <span>{command.title}</span>
                <span class="d-inline-flex gap-1">
                  {#each command.shortcuts as shortcut}
                    <kbd>{shortcut}</kbd>
                  {/each}
                </span>
              </li>
            {/each}
          </ul>
        </div>
      {/each}
      <p class="f6 color-fg-muted mb-0">
        Saved reply insertion commands are dynamic and appear in the command palette as
        <code>Insert: &lt;name&gt;</code>.
      </p>
    </div>
  </section>
</main>
