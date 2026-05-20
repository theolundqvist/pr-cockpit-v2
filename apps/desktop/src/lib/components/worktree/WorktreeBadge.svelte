<script lang="ts">
  import type { WorktreeView } from '$lib/ipc/bindings';

  export let worktree: WorktreeView;
</script>

<span class={`Label ${worktree.dirty ? 'Label--attention' : 'Label--secondary'}`}>
  {worktree.branch} • {worktree.dirty ? 'dirty' : 'clean'} • ↑{worktree.ahead}/↓{worktree.behind}
</span>
