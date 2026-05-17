#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .run(tauri::generate_context!())
        .expect("error while running desktop app");
}

pub mod db;

#[cfg(test)]
mod tests {
    #[test]
    fn package_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "desktop");
    }
}
