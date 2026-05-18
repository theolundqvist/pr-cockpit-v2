use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tauri::Manager;

use crate::api::{AccountResolver, GithubClient, ResolvedAccountEndpoints};
use crate::auth::token_client::TokenClient;
use crate::auth::AuthService;
use crate::db::Db;
use crate::ipc::worktree::{watcher::WatchBackend, WorktreeService};
use crate::mutations::{MutationEngine, NetworkMonitor};
use crate::notify::{
    dispatcher::TauriNotificationSender, NotificationEngine, NotificationEventEmitter,
};

#[derive(Debug, Clone)]
struct InboxSeedState {
    json: String,
}

#[derive(Clone)]
struct AuthAccountResolver {
    auth: Arc<AuthService>,
}

#[async_trait]
impl AccountResolver for AuthAccountResolver {
    async fn resolve(&self, account_id: &str) -> Result<ResolvedAccountEndpoints> {
        let (account, secret) = self.auth.account_secret_by_id(account_id).await?;
        let endpoints = self.auth.endpoint_config_for_host(&account.host);
        Ok(ResolvedAccountEndpoints {
            locator: auth::AccountLocator {
                host: account.host,
                login: account.login,
            },
            token: secret.access_token,
            api_base_url: endpoints.api_base_url,
            graphql_url: endpoints.graphql_url,
        })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta_builder = ipc::specta_builder::<tauri::Wry>();
    let invoke_handler = specta_builder.invoke_handler();

    #[cfg(debug_assertions)]
    if let Err(error) = ipc::export_default_bindings() {
        panic!("failed to generate IPC bindings: {error:#}");
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "desktop=info,tauri=warn".to_string()),
        )
        .try_init();

    tauri::Builder::default()
        .setup(move |app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .context("resolving app data directory")?;

            let db = Arc::new(
                tauri::async_runtime::block_on(Db::open(&data_dir))
                    .context("opening sqlite database")?,
            );
            let auth_service =
                Arc::new(AuthService::new(Arc::clone(&db)).context("building auth service")?);
            let sync_state = Arc::new(sync::SyncTierStateStore::default());
            let account_resolver = Arc::new(AuthAccountResolver {
                auth: Arc::clone(&auth_service),
            });
            let github = Arc::new(GithubClient::with_account_resolver(
                TokenClient::from_auth_service(Arc::clone(&auth_service)),
                Arc::clone(&db),
                account_resolver,
                "https://api.github.com/zen".to_string(),
            ));
            let cache_emitter = Arc::new(ipc::TauriCacheInvalidationEmitter::new(
                app.handle().clone(),
            ));
            let worktree_emitter = Arc::new(ipc::TauriWorktreeEventEmitter::new(app.handle().clone()));
            let network_monitor = NetworkMonitor::start(github.probe_url());
            let mutation_engine = Arc::new(
                MutationEngine::new(Arc::clone(&db), github.as_ref().clone())
                    .with_cache_emitter(cache_emitter.clone())
                    .with_network_monitor(network_monitor.clone()),
            );
            let worktree_service = Arc::new(WorktreeService::new(
                Arc::clone(&db),
                worktree_emitter,
                WatchBackend::default(),
            ));
            let notification_sender = Arc::new(TauriNotificationSender::new(app.handle().clone()));
            let notification_emitter: Arc<dyn NotificationEventEmitter> = cache_emitter.clone();
            let notification_engine = Arc::new(
                tauri::async_runtime::block_on(NotificationEngine::new(
                    Arc::clone(&db),
                    notification_sender,
                    notification_emitter,
                ))
                .context("building notification engine")?,
            );
            let inbox_seed = tauri::async_runtime::block_on(ipc::ipc_init_inbox_impl(
                db.as_ref(),
                auth_service.as_ref(),
                sync_state.as_ref(),
            ))
            .unwrap_or_else(|_| ipc::InitInboxResponse {
                active_account_id: None,
                accounts: auth::AccountsListResponse {
                    active: None,
                    accounts: Vec::new(),
                },
                subscriptions: Vec::new(),
                inbox: Vec::new(),
                status: None,
            });
            let inbox_seed_state = InboxSeedState {
                json: serde_json::to_string(&inbox_seed).unwrap_or_else(|_| "null".to_string()),
            };

            app.manage(Arc::clone(&db));
            app.manage(Arc::clone(&auth_service));
            app.manage(Arc::clone(&sync_state));
            app.manage(Arc::clone(&mutation_engine));
            app.manage(Arc::clone(&worktree_service));
            app.manage(Arc::clone(&notification_engine));
            app.manage(Arc::clone(&github));
            app.manage(inbox_seed_state);

            {
                let emitter = Arc::clone(&cache_emitter);
                let mut events_rx = mutation_engine.subscribe();
                tauri::async_runtime::spawn(async move {
                    loop {
                        match events_rx.recv().await {
                            Ok(event) => ipc::fanout_mutation_event(&emitter, event),
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        }
                    }
                });
            }

            {
                let notification_engine = Arc::clone(&notification_engine);
                let mutation_rx = mutation_engine.subscribe();
                let sync_rx = sync::subscribe_reconciled_events();
                notification_engine.spawn(mutation_rx, sync_rx);
            }

            {
                let emitter = Arc::clone(&cache_emitter);
                let db = Arc::clone(&db);
                let mut net_rx = network_monitor.subscribe();
                tauri::async_runtime::spawn(async move {
                    loop {
                        if net_rx.changed().await.is_err() {
                            break;
                        }
                        let state = net_rx.borrow().clone();
                        let account_rows = db.list_auth_accounts().await.unwrap_or_default();
                        if account_rows.is_empty() {
                            emitter.emit_network_changed("unknown", state.clone());
                            continue;
                        }
                        for account in account_rows {
                            emitter.emit_network_changed(&account.id, state.clone());
                        }
                    }
                });
            }

            {
                let worktree_service = Arc::clone(&worktree_service);
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = worktree_service.start().await {
                        tracing::warn!(target: "worktree", error = %error, "worktree startup failed");
                    }
                });
            }

            specta_builder.mount_events(app);
            Ok(())
        })
        .on_page_load(|window, _payload| {
            let inbox_seed = window.state::<InboxSeedState>();
            let script = format!(
                "window.__INBOX_SEED__ = {}; window.dispatchEvent(new Event('inbox-seed-ready'));",
                inbox_seed.json
            );
            let _ = window.eval(&script);
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(invoke_handler)
        .run(tauri::generate_context!())
        .expect("error while running desktop app");
}

pub mod api;
pub mod auth;
pub mod db;
pub mod ipc;
pub mod mutations;
pub mod notify;
pub mod range_diff;
pub mod render;
pub mod storage;
pub mod sync;
pub use sync::{shutdown as sync_shutdown, start as sync_start};
pub static RENDERER_VERSION: &str = "m1-renderer-v1";

#[cfg(test)]
mod tests {
    #[test]
    fn package_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "desktop");
    }
}
