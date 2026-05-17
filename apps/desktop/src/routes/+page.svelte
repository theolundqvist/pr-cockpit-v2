<script lang="ts">
  import inboxIcon from '@primer/octicons/build/svg/inbox-24.svg?raw';
</script>

<main class="cockpit-shell">
  <div class="Box">
    <div class="Box-header d-flex flex-items-center gap-2">
      <span class="color-fg-muted d-flex" aria-hidden="true">
        {@html inboxIcon}
      </span>
      <h1 class="f3 text-normal m-0">Pull Request Inbox</h1>
    </div>
    <div class="Box-body">
      <p class="color-fg-muted mb-2">
        No pull requests synced yet.
      </p>
      <p class="f6 color-fg-subtle mb-0">
        This static scaffold is ready for downstream M1 read models.
      </p>
    </div>
  </div>
</main>
