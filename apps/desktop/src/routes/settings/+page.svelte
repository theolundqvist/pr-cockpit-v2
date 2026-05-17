<script lang="ts">
  import NotificationsSettings from '$lib/components/settings/Notifications.svelte';
  import { activeAccountIdStore } from '$lib/state/cockpit';
</script>

<main class="px-3 py-3">
  <section class="Box">
    <div class="Box-header">
      <h1 class="f3 m-0">Notification settings</h1>
    </div>
    <div class="Box-body">
      <NotificationsSettings accountId={$activeAccountIdStore} />
    </div>
  </section>
</main>
