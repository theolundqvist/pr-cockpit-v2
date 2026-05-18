# PR Cockpit webhook relay (optional)

This relay exists for the last 10% of perceived notification latency. Polling is
still the primary, resilient source of truth; this Worker only shortens the
last-mile event delivery path to a local desktop receiver.

## Security + deployment model

- **Self-deploy only.** This repository does not ship or operate a hosted relay
  service.
- **No SaaS default.** You deploy your own Worker in your own Cloudflare
  account.
- **Two independent secrets:**
  - `GITHUB_WEBHOOK_SECRET` validates GitHub -> relay traffic.
  - `RELAY_FORWARD_SECRET` signs relay -> desktop traffic.
- **Replay protection** is enforced on the desktop side with nonce + timestamp.

Optional multi-destination routing can be enabled by uncommenting the
`RELAY_DESTINATIONS` KV binding in `wrangler.toml`.

## Deploy (self-hosted)

1. Install Wrangler:
   - `npm i -g wrangler`
   - or `pnpm add -g wrangler`
2. Login: `wrangler login`
3. Install relay dependencies:
   - `cd relay`
   - `pnpm install`
4. Set secrets:
   - `wrangler secret put GITHUB_WEBHOOK_SECRET`
   - `wrangler secret put RELAY_FORWARD_SECRET`
   - `wrangler secret put RELAY_DESTINATION_URL`
     - Example destination:
       `https://my-tunnel.example.com/webhook`
5. Deploy: `pnpm deploy`
6. Add the deployed Worker URL as a GitHub repository webhook and use the same
   `GITHUB_WEBHOOK_SECRET`.

## Revoke

1. Immediately disable inbound verification:
   - `wrangler secret delete GITHUB_WEBHOOK_SECRET`
2. Remove the Worker:
   - `wrangler delete pr-cockpit-relay`
3. Remove the GitHub webhook from repository settings.
4. Restart the desktop app to clear cached relay state.

## Desktop receiver integration

The desktop side hosts a local HTTP endpoint at:

- `apps/desktop/src-tauri/src/relay/mod.rs`

The module exposes IPC command:

- `relay_local_url() -> String`

Use the returned URL as `RELAY_DESTINATION_URL` for your Worker. The receiver
verifies `RELAY_FORWARD_SECRET` signatures and applies replay-window checks via
nonce + timestamp validation.

## CI/verification

- `pnpm test` runs relay unit tests.
- `pnpm exec wrangler deploy --dry-run` verifies Worker compile/deploy readiness
  without publishing.
- CI should keep both commands green for this package.
