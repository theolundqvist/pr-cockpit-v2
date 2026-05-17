use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tauri_plugin_notification::NotificationExt;

pub trait OsNotificationSender: Send + Sync {
    fn send(&self, title: &str, body: &str) -> Result<()>;
}

#[derive(Clone)]
pub struct TauriNotificationSender<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> TauriNotificationSender<R> {
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: tauri::Runtime> OsNotificationSender for TauriNotificationSender<R> {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        self.app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .with_context(|| format!("sending OS notification `{title}`"))?;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct RecordingNotificationSender {
    sent: Arc<Mutex<Vec<(String, String)>>>,
}

impl RecordingNotificationSender {
    pub fn sent_count(&self) -> usize {
        self.sent
            .lock()
            .map(|records| records.len())
            .unwrap_or_default()
    }

    pub fn records(&self) -> Vec<(String, String)> {
        self.sent
            .lock()
            .map(|records| records.clone())
            .unwrap_or_default()
    }
}

impl OsNotificationSender for RecordingNotificationSender {
    fn send(&self, title: &str, body: &str) -> Result<()> {
        if let Ok(mut sent) = self.sent.lock() {
            sent.push((title.to_string(), body.to_string()));
        }
        Ok(())
    }
}
