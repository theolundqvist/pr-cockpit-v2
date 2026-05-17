use std::sync::Arc;

use anyhow::Context;
use tauri::Manager;

use crate::auth::AuthService;
use crate::db::Db;

#[derive(Debug, Clone)]
struct InboxSeedState {
    json: String,
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

            app.manage(db);
            app.manage(auth_service);
            app.manage(sync_state);
            app.manage(inbox_seed_state);

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
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(invoke_handler)
        .run(tauri::generate_context!())
        .expect("error while running desktop app");
}

pub mod api;
pub mod auth;
pub mod db;
pub mod ipc;
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
