use std::sync::Arc;

use anyhow::Context;
use tauri::Manager;

use crate::auth::AuthService;
use crate::db::Db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "desktop=info,tauri=warn".to_string()),
        )
        .try_init();

    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .context("resolving app data directory")?;
            let db = tauri::async_runtime::block_on(Db::open(&data_dir))
                .context("opening sqlite database")?;
            let auth_service =
                Arc::new(AuthService::new(Arc::new(db)).context("building auth service")?);
            app.manage(auth_service);
            Ok(())
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            auth::auth_detect_gh_token,
            auth::auth_oauth_device_start,
            auth::auth_oauth_device_poll,
            auth::auth_pat_save,
            auth::auth_list_accounts,
            auth::auth_switch_account,
            auth::auth_remove_account,
            auth::auth_refresh
        ])
        .run(tauri::generate_context!())
        .expect("error while running desktop app");
}

pub mod auth;
pub mod db;

#[cfg(test)]
mod tests {
    #[test]
    fn package_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "desktop");
    }
}
