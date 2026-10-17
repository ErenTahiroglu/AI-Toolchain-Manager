//! Unexpected-failure capture. A panic hook leaves a small redacted marker file; the next start
//! ingests it into the diagnostics log and audit chain, then deletes it. Nothing is sent anywhere.

use crate::redact;
use std::{path::PathBuf, sync::OnceLock};

const MARKER: &str = "crash-pending.txt";
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

fn write_marker(dir: &std::path::Path, summary: &str) {
    let _ = std::fs::write(dir.join(MARKER), redact::sanitize(summary, 400));
}

pub fn install_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = DATA_DIR.get() {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_string());
            let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            write_marker(dir, &format!("panic at {place}: {message}"));
        }
        previous(info);
    }));
}

/// Record a startup failure that happens before any window exists (for example a missing WebView2).
pub fn record_fatal(dir: &std::path::Path, what: &str) {
    write_marker(dir, &format!("fatal startup error: {what}"));
}

/// Returns and removes the pending marker, if any.
pub fn take_pending(dir: &std::path::Path) -> Option<String> {
    let path = dir.join(MARKER);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    Some(redact::sanitize(&text, 400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_redacted_and_consumed_once() {
        let dir = tempfile::tempdir().unwrap();
        record_fatal(dir.path(), "boom at C:\\Users\\dummy\\x.rs token=DUMMYSECRET99");
        let text = take_pending(dir.path()).unwrap();
        assert!(!text.contains("dummy") && !text.contains("DUMMYSECRET99"));
        assert!(take_pending(dir.path()).is_none());
    }
}
