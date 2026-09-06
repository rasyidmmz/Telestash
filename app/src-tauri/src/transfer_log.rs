use crate::failure_classifier::classify_failure;
use serde::Serialize;
use std::sync::{Mutex, OnceLock};

const MAX_TRANSFER_LOGS: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct TransferLogEntry {
    pub time: String,
    pub source: String,
    pub category: String,
    pub message: String,
    pub details: Option<String>,
    pub level: &'static str, // "error" | "info"
}

static TRANSFER_LOGS: OnceLock<Mutex<Vec<TransferLogEntry>>> = OnceLock::new();

pub(crate) fn record_transfer_log(
    source: impl Into<String>,
    message: impl Into<String>,
    details: Option<String>,
) {
    let message = message.into();
    let details = details;
    let category = classify_failure(&message, details.as_deref()).to_string();
    let entry = TransferLogEntry {
        time: chrono::Utc::now().to_rfc3339(),
        source: source.into(),
        category,
        message,
        details,
        level: "error",
    };
    let mut logs = logs().lock().unwrap();
    logs.insert(0, entry);
    logs.truncate(MAX_TRANSFER_LOGS);
}

/// Success-path logging: skips failure classification so completed
/// operations are shown as info instead of being styled as errors.
pub(crate) fn record_transfer_success(
    source: impl Into<String>,
    message: impl Into<String>,
    details: Option<String>,
) {
    let entry = TransferLogEntry {
        time: chrono::Utc::now().to_rfc3339(),
        source: source.into(),
        category: "success".to_string(),
        message: message.into(),
        details,
        level: "info",
    };
    let mut logs = logs().lock().unwrap();
    logs.insert(0, entry);
    logs.truncate(MAX_TRANSFER_LOGS);
}

pub(crate) fn transfer_logs() -> Vec<TransferLogEntry> {
    logs().lock().unwrap().clone()
}

pub(crate) fn clear_transfer_logs() {
    logs().lock().unwrap().clear();
}

#[tauri::command]
pub fn cmd_get_transfer_logs() -> Vec<TransferLogEntry> {
    transfer_logs()
}

#[tauri::command]
pub fn cmd_clear_transfer_logs() {
    clear_transfer_logs();
}

fn logs() -> &'static Mutex<Vec<TransferLogEntry>> {
    TRANSFER_LOGS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    // The log store is process-global; serialize the tests that touch it so
    // parallel test threads cannot steal capped slots from each other.
    static LOG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn keeps_newest_entries_first_and_caps_old_entries() {
        let _guard = LOG_TEST_LOCK.lock().unwrap();
        super::clear_transfer_logs();

        for i in 0..105 {
            super::record_transfer_log("upload", format!("entry {i}"), None);
        }

        let logs = super::transfer_logs();
        assert_eq!(logs.len(), 100);
        assert_eq!(logs[0].message, "entry 104");
        assert_eq!(logs[99].message, "entry 5");
    }

    #[test]
    fn classifies_logged_transfer_errors() {
        let _guard = LOG_TEST_LOCK.lock().unwrap();
        super::clear_transfer_logs();

        super::record_transfer_log("upload", "request error: read 0 bytes", None);

        let logs = super::transfer_logs();
        assert_eq!(logs[0].category, "network/transport");
        assert_eq!(logs[0].level, "error");
    }

    #[test]
    fn success_entries_skip_failure_classification() {
        let _guard = LOG_TEST_LOCK.lock().unwrap();
        super::clear_transfer_logs();

        super::record_transfer_success(
            "Subtitle Attach",
            "Subtitle movie.en.srt attached to video message 42",
            Some("folder_id: 7".to_string()),
        );

        let logs = super::transfer_logs();
        assert_eq!(logs[0].level, "info");
        assert_eq!(logs[0].category, "success");
    }
}
