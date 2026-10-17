use serde::Serialize;

/// Error returned to the WebView. `message` is always a fixed, path-free sentence;
/// raw library errors are never forwarded (they can contain local paths).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn db_unavailable() -> Self {
        Self::new("DB_UNAVAILABLE", "Yerel veritabanı açılamadı. Uygulama güvenli (bozulmuş) modda; değişiklik yapılamaz.")
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self {
        AppError::new("DB_ERROR", "Veritabanı işlemi başarısız oldu.")
    }
}

impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        AppError::new("JSON_ERROR", "Veri okunamadı veya biçimi geçersiz.")
    }
}

pub type AppResult<T> = Result<T, AppError>;
