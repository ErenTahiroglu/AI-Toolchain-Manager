//! Execution gate. Phase 1B has no executor at all: no command, adapter or button can run, install
//! or verify anything. This module is the single place that states why, and it is the contract a
//! later, separately reviewed phase must satisfy before any run becomes possible.
//!
//! For active security testing (Strix) the gate additionally demands a complete, scoped, written
//! authorization. Even a perfectly valid authorization is *not* enough in Phase 1B.

use crate::{catalog::CatalogEntry, error::AppError};

pub const EXECUTION_NOT_AVAILABLE: &str = "EXECUTION_NOT_AVAILABLE_PHASE_1B";
pub const AUTHORIZATION_REQUIRED: &str = "SCOPED_AUTHORIZATION_REQUIRED";
pub const AUTHORIZATION_INVALID: &str = "AUTHORIZATION_INVALID";

const MAX_DURATION_MINUTES: u32 = 480;

/// Contract for a later phase: constructed only by tests in Phase 1B because nothing can run yet.
#[allow(dead_code)]
/// Everything a human must have decided in writing before an active security test may even be considered.
#[derive(Debug, Clone)]
pub struct ScopedAuthorization {
    /// An exact host or URL the user owns or may test. Wildcards and broad ranges are rejected.
    pub target: String,
    /// `test`, `staging` or `local`. Never `production`.
    pub environment: String,
    pub duration_minutes: u32,
    pub max_requests_per_minute: u32,
    pub provider_budget_usd: f64,
    /// Reference (not a secret) to the written permission, such as a ticket or document id.
    pub written_permission_ref: String,
    pub user_confirmed_in_person: bool,
}

fn broad_target(target: &str) -> bool {
    let t = target.trim().to_ascii_lowercase();
    t.is_empty()
        || t.contains('*')
        || t.contains(',')
        || t.contains(' ')
        || (t.contains('/') && t.ends_with("/0"))
        || matches!(t.as_str(), "0.0.0.0" | "localhost/*" | "any" | "all" | "internet" | "*.*")
        || t.starts_with("0.0.0.0/")
}

fn validate(auth: &ScopedAuthorization) -> Result<(), &'static str> {
    if broad_target(&auth.target) {
        return Err("hedef belirli ve tek olmalı (joker karakter/aralık yok)");
    }
    if !matches!(auth.environment.as_str(), "test" | "staging" | "local") {
        return Err("ortam yalnızca test, staging veya local olabilir (üretim yasak)");
    }
    if auth.duration_minutes == 0 || auth.duration_minutes > MAX_DURATION_MINUTES {
        return Err("süre sınırı gerekli ve en çok 8 saat olabilir");
    }
    if auth.max_requests_per_minute == 0 || auth.max_requests_per_minute > 600 {
        return Err("istek hızı sınırı gerekli");
    }
    if !(auth.provider_budget_usd.is_finite() && auth.provider_budget_usd > 0.0 && auth.provider_budget_usd <= 1000.0) {
        return Err("sağlayıcı bütçe sınırı gerekli");
    }
    if auth.written_permission_ref.trim().len() < 3 {
        return Err("yazılı izin referansı gerekli");
    }
    if !auth.user_confirmed_in_person {
        return Err("kullanıcı onayı gerekli");
    }
    Ok(())
}

/// Always `Err` in Phase 1B. The error code tells *why*, in a stable order of precedence.
pub fn check_execution(entry: &CatalogEntry, action: &str, authorization: Option<&ScopedAuthorization>) -> Result<(), AppError> {
    if entry.removed_at.is_some() {
        return Err(AppError::new("ENTRY_REMOVED", "Bu kayıt katalogdan kaldırılmış."));
    }
    if entry.execution_policy == "blocked-until-scoped-authorization" {
        match authorization {
            None => {
                return Err(AppError::new(
                    AUTHORIZATION_REQUIRED,
                    "Aktif güvenlik testi için hedef, ortam, süre, hız, bütçe ve yazılı izin içeren kapsamlı yetki gerekir.",
                ))
            }
            Some(auth) => {
                if let Err(why) = validate(auth) {
                    return Err(AppError::new(AUTHORIZATION_INVALID, format!("Yetki geçersiz: {why}.")));
                }
            }
        }
    }
    let _ = action; // install | run | update | verify — none is available in Phase 1B
    Err(AppError::new(EXECUTION_NOT_AVAILABLE, "Faz 1B'de hiçbir araç kurulamaz, çalıştırılamaz veya doğrulanamaz."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    fn entry(id: &str) -> CatalogEntry {
        let conn = crate::db::open_memory().unwrap();
        catalog::seed_all(&conn).unwrap();
        catalog::get_entry(&conn, id).unwrap()
    }

    fn good() -> ScopedAuthorization {
        ScopedAuthorization {
            target: "staging.example.invalid".into(),
            environment: "staging".into(),
            duration_minutes: 60,
            max_requests_per_minute: 30,
            provider_budget_usd: 5.0,
            written_permission_ref: "DOC-123".into(),
            user_confirmed_in_person: true,
        }
    }

    #[test]
    fn strix_without_authorization_is_refused_with_the_authorization_reason() {
        let e = entry("strix");
        for action in ["run", "scan", "install", "verify"] {
            assert_eq!(check_execution(&e, action, None).unwrap_err().code, AUTHORIZATION_REQUIRED, "{action}");
        }
    }

    #[test]
    fn incomplete_broad_or_production_authorizations_are_invalid() {
        let e = entry("strix");
        let mutate: [fn(&mut ScopedAuthorization); 10] = [
            |a| a.target = "*".into(),
            |a| a.target = "*.example.invalid".into(),
            |a| a.target = "a.invalid, b.invalid".into(),
            |a| a.target = "10.0.0.0/0".into(),
            |a| a.target = String::new(),
            |a| a.environment = "production".into(),
            |a| a.duration_minutes = 0,
            |a| a.provider_budget_usd = f64::NAN,
            |a| a.written_permission_ref = " ".into(),
            |a| a.user_confirmed_in_person = false,
        ];
        for (i, m) in mutate.iter().enumerate() {
            let mut a = good();
            m(&mut a);
            assert_eq!(check_execution(&e, "run", Some(&a)).unwrap_err().code, AUTHORIZATION_INVALID, "case {i}");
        }
        let mut too_fast = good();
        too_fast.max_requests_per_minute = 100_000;
        assert_eq!(check_execution(&e, "run", Some(&too_fast)).unwrap_err().code, AUTHORIZATION_INVALID);
        let mut too_long = good();
        too_long.duration_minutes = 100_000;
        assert_eq!(check_execution(&e, "run", Some(&too_long)).unwrap_err().code, AUTHORIZATION_INVALID);
    }

    #[test]
    fn even_a_perfect_authorization_cannot_run_anything_in_phase_1b() {
        assert_eq!(check_execution(&entry("strix"), "run", Some(&good())).unwrap_err().code, EXECUTION_NOT_AVAILABLE);
    }

    #[test]
    fn every_other_entry_is_also_not_executable() {
        for id in ["agent-browser", "gitleaks", "anthropic-frontend-design", "agentic-awesome-skills", "superpowers"] {
            for action in ["run", "install", "update", "verify"] {
                assert_eq!(check_execution(&entry(id), action, Some(&good())).unwrap_err().code, EXECUTION_NOT_AVAILABLE, "{id} {action}");
            }
        }
    }
}
