use crate::{
    error::{AppError, AppResult},
    health::{HealthReport, PreviousShutdown, TrayState},
};
use rusqlite::Connection;
use std::sync::{atomic::AtomicU32, Mutex};

pub struct AppState {
    /// `None` means the database could not be opened: the app runs degraded and refuses mutations.
    pub db: Mutex<Option<Connection>>,
    pub startup_error: Option<AppError>,
    pub data_dir: String,
    pub tray: Mutex<TrayState>,
    pub previous_shutdown: PreviousShutdown,
    pub last_report: Mutex<Option<HealthReport>>,
    /// Caps UI error reports per session so a render loop cannot flood the log.
    pub ui_error_count: AtomicU32,
}

impl AppState {
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard =
            self.db.lock().map_err(|_| AppError::new("STATE_POISONED", "Uygulama içi durum bozuldu; uygulamayı yeniden başlatın."))?;
        match guard.as_ref() {
            Some(conn) => f(conn),
            None => Err(AppError::db_unavailable()),
        }
    }

    pub fn tray_state(&self) -> TrayState {
        self.tray.lock().map(|t| *t).unwrap_or(TrayState::Failed)
    }
}
