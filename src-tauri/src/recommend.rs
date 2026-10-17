//! Deterministic contextual recommendation prototype (ADR-0002).
//!
//! No model is called and nothing is sent anywhere: relevance comes from fixed keyword rules over the
//! user's task text plus the *names* of files the caller says are involved. Nothing is read from disk.
//! A recommendation never installs, enables in a host, or changes configuration; at most the user
//! records a *desired* per-project preference.
//!
//! Gates, in order: catalog state → compatibility → earlier decision → availability hints →
//! once-per-task de-duplication → per-task cap. Every refusal is reported with a reason.

use crate::{
    audit::now_utc,
    catalog::{self, CatalogEntry},
    error::{AppError, AppResult},
    projects, skills,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MAX_PER_TASK: usize = 2;
const MAX_TASK_CHARS: usize = 2000;
const MAX_FILES: usize = 50;
const MAX_FILE_NAME_CHARS: usize = 200;

pub const FRONTEND_DESIGN: &str = "anthropic-frontend-design";
pub const AGENT_BROWSER: &str = "agent-browser";
pub const SKILLS_COLLECTION: &str = "agentic-awesome-skills";
pub const STRIX: &str = "strix";

// ---------------------------------------------------------------------------------------------
// Text handling
// ---------------------------------------------------------------------------------------------

fn fold(c: char) -> char {
    match c {
        'İ' | 'I' | 'ı' => 'i',
        'ş' | 'Ş' => 's',
        'ğ' | 'Ğ' => 'g',
        'ü' | 'Ü' => 'u',
        'ö' | 'Ö' => 'o',
        'ç' | 'Ç' => 'c',
        other => other,
    }
}

/// Lower-case, Turkish-folded alphanumeric tokens. Whole-token matching means `css` does not
/// match `access` and `ui` does not match `build`.
pub fn tokens(text: &str) -> Vec<String> {
    let folded: String = text.chars().map(fold).flat_map(char::to_lowercase).collect();
    folded.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty() && t.len() <= 40).map(str::to_string).collect()
}

fn has(tokens: &[String], word: &str) -> bool {
    tokens.iter().any(|t| t == word)
}

fn count_of(tokens: &[String], words: &[&str]) -> usize {
    words.iter().filter(|w| has(tokens, w)).count()
}

fn has_phrase(tokens: &[String], phrase: &[&str]) -> bool {
    tokens.windows(phrase.len()).any(|w| w.iter().zip(phrase).all(|(a, b)| a == b))
}

fn any_phrase(tokens: &[String], phrases: &[&[&str]]) -> bool {
    phrases.iter().any(|p| has_phrase(tokens, p))
}

// ---------------------------------------------------------------------------------------------
// Intent classification
// ---------------------------------------------------------------------------------------------

const STRONG_UI: &[&str] = &[
    "html",
    "css",
    "scss",
    "sass",
    "tailwind",
    "jsx",
    "tsx",
    "react",
    "vue",
    "svelte",
    "layout",
    "responsive",
    "ui",
    "ux",
    "frontend",
    "arayuz",
    "styling",
    "navbar",
    "button",
    "buton",
];
const STRONG_UI_PHRASES: &[&[&str]] = &[
    &["front", "end"],
    &["landing", "page"],
    &["web", "page"],
    &["web", "sayfasi"],
    &["user", "interface"],
    &["kullanici", "arayuzu"],
    &["dark", "mode"],
    &["karanlik", "mod"],
    &["css", "grid"],
];
const WEAK_UI: &[&str] = &[
    "design",
    "tasarim",
    "tasarla",
    "component",
    "components",
    "bilesen",
    "page",
    "sayfa",
    "style",
    "stil",
    "theme",
    "tema",
    "color",
    "renk",
    "dashboard",
    "animation",
    "animasyon",
    "typography",
];
const BACKEND: &[&str] = &[
    "backend",
    "database",
    "db",
    "sql",
    "migration",
    "migrations",
    "endpoint",
    "endpoints",
    "grpc",
    "orm",
    "cron",
    "queue",
    "worker",
    "veritabani",
    "sunucu",
    "server",
    "kubernetes",
    "terraform",
];
const UI_EXTENSIONS: &[&str] = &["html", "htm", "css", "scss", "sass", "less", "jsx", "tsx", "vue", "svelte"];

const BROWSER_WORDS: &[&str] =
    &["browser", "tarayici", "tarayicida", "tarayicidan", "screenshot", "playwright", "puppeteer", "selenium", "cypress"];
const TEST_WORDS: &[&str] =
    &["test", "testing", "tests", "verify", "validate", "dogrula", "dogrulama", "qa", "regression", "check", "kontrol", "dene"];
const WEBISH: &[&str] = &["web", "website", "webapp", "site", "sayfa", "ui", "frontend", "browser", "tarayici"];
const SKILL_WORDS: &[&str] = &["skill", "skills", "yetenek", "yetenekler"];
const SEARCH_WORDS: &[&str] = &[
    "find",
    "search",
    "ara",
    "bul",
    "discover",
    "look",
    "recommend",
    "oner",
    "suggest",
    "reusable",
    "existing",
    "varmi",
    "mevcut",
    "hazir",
];
const ASSESSMENT_PHRASES: &[&[&str]] = &[
    &["penetration", "test"],
    &["penetration", "testing"],
    &["sizma", "testi"],
    &["red", "team"],
    &["security", "assessment"],
    &["guvenlik", "degerlendirmesi"],
    &["vulnerability", "assessment"],
    &["zafiyet", "taramasi"],
];
const SECURITY_WORDS: &[&str] = &["security", "guvenlik", "vulnerability", "zafiyet", "xss", "csrf", "exploit", "injection"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Intents {
    pub frontend: bool,
    pub frontend_signals: Vec<String>,
    pub ui_files: usize,
    pub backend_terms: bool,
    pub browser_test: bool,
    pub skill_search: bool,
    pub security_assessment: bool,
    pub security_keyword: bool,
}

fn extension(file_name: &str) -> String {
    file_name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

pub fn classify(task_text: &str, evidence_files: &[String]) -> Intents {
    let t = tokens(task_text);
    let ui_files = evidence_files.iter().filter(|f| UI_EXTENSIONS.contains(&extension(f).as_str())).count();
    let strong: Vec<&str> = STRONG_UI.iter().copied().filter(|w| has(&t, w)).collect();
    let strong_phrases = STRONG_UI_PHRASES.iter().filter(|p| has_phrase(&t, p)).count();
    let weak = count_of(&t, WEAK_UI);
    let backend_terms = count_of(&t, BACKEND) > 0;
    let strong_total = strong.len() + strong_phrases;
    // Task intent is required. File evidence only reinforces a weak intent; it never creates one,
    // and a CSS file sitting in a backend repository means nothing by itself.
    let frontend = !(backend_terms && strong_total == 0) && (strong_total >= 1 || weak >= 2 || (weak >= 1 && ui_files >= 1));
    let mut frontend_signals: Vec<String> = Vec::new();
    if frontend {
        frontend_signals.extend(strong.iter().map(|s| format!("görev ifadesi: {s}")));
        if strong_phrases > 0 {
            frontend_signals.push("görev ifadesi: arayüz tasarımı".to_string());
        }
        if strong_total == 0 {
            frontend_signals.push("görev ifadesi: tasarım/bileşen/stil".to_string());
        }
        if ui_files > 0 {
            frontend_signals.push(format!("ilgili arayüz dosyası: {ui_files}"));
        }
    }
    let browser_terms = count_of(&t, BROWSER_WORDS) > 0 || has_phrase(&t, &["ekran", "goruntusu"]);
    let browser_test = (browser_terms && count_of(&t, TEST_WORDS) > 0)
        || has_phrase(&t, &["visual", "regression"])
        || ((has(&t, "e2e") || has_phrase(&t, &["end", "to", "end"]) || has_phrase(&t, &["uctan", "uca"])) && count_of(&t, WEBISH) > 0);
    let skill_search = (count_of(&t, SKILL_WORDS) > 0 && count_of(&t, SEARCH_WORDS) > 0) || has_phrase(&t, &["is", "there", "a", "skill"]);
    let security_assessment = has(&t, "pentest") || has(&t, "pentesting") || any_phrase(&t, ASSESSMENT_PHRASES);
    let security_keyword = count_of(&t, SECURITY_WORDS) > 0;
    Intents { frontend, frontend_signals, ui_files, backend_terms, browser_test, skill_search, security_assessment, security_keyword }
}

// ---------------------------------------------------------------------------------------------
// Input / output
// ---------------------------------------------------------------------------------------------

/// A hint about what is *already* available in the host. Real detection does not exist yet, so the
/// caller (a synthetic scenario or a test) supplies it. It is never written to the database.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub entry_id: String,
    /// `global` or `project`
    pub scope: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub project_alias: String,
    pub task_id: String,
    pub task_text: String,
    #[serde(default)]
    pub evidence_files: Vec<String>,
    pub agent_surface: String,
    pub platform_target: String,
    /// `start` (default) or `late` (new domain discovered mid-task; at most one per task).
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub availability: Vec<Availability>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub entry_id: String,
    pub display_name: String,
    /// `suggest`: the user may record "enable for this project". `informational`: shown only; cannot be enabled.
    pub level: &'static str,
    pub can_enable: bool,
    pub rationale: String,
    pub impact: String,
    pub signals: Vec<String>,
    pub notes: Vec<String>,
    pub skill_candidates: Vec<skills::SkillCandidate>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suppressed {
    pub entry_id: String,
    pub reason_code: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub task_id: String,
    pub phase: String,
    pub recommendations: Vec<Recommendation>,
    pub suppressed: Vec<Suppressed>,
    pub disclaimer: &'static str,
}

const DISCLAIMER: &str = "Bu bir öneri önizlemesidir. Hiçbir şey kurulmadı, çalıştırılmadı ve hiçbir ajan ayarı değiştirilmedi; hiçbir yapay zekâ modeli çağrılmadı.";

fn suppressed(entry_id: &str, reason_code: &'static str, reason: &str) -> Suppressed {
    Suppressed { entry_id: entry_id.to_string(), reason_code, reason: reason.to_string() }
}

struct Hit {
    entry_id: &'static str,
    signals: Vec<String>,
    rationale: &'static str,
    impact: &'static str,
}

fn validate(input: &TaskInput) -> AppResult<(String, Vec<String>, &'static str)> {
    let bad_id = input.task_id.is_empty()
        || input.task_id.len() > 64
        || !input.task_id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_');
    if bad_id {
        return Err(AppError::new("TASK_ID_INVALID", "Görev kimliği 1-64 karakter olmalı (harf, rakam, - veya _)."));
    }
    if !projects::SURFACES.contains(&input.agent_surface.as_str()) {
        return Err(AppError::new("SURFACE_INVALID", "Bilinmeyen ajan yüzeyi."));
    }
    if !projects::PLATFORMS.contains(&input.platform_target.as_str()) {
        return Err(AppError::new("PLATFORM_INVALID", "Bilinmeyen platform."));
    }
    let phase = match input.phase.as_deref() {
        None | Some("start") => "start",
        Some("late") => "late",
        Some(_) => return Err(AppError::new("PHASE_INVALID", "Aşama 'start' veya 'late' olmalı.")),
    };
    let text: String = input.task_text.chars().take(MAX_TASK_CHARS).collect();
    let files: Vec<String> = input.evidence_files.iter().take(MAX_FILES).map(|f| f.chars().take(MAX_FILE_NAME_CHARS).collect()).collect();
    Ok((text, files, phase))
}

// ---------------------------------------------------------------------------------------------
// Gates
// ---------------------------------------------------------------------------------------------

struct Passed {
    entry: CatalogEntry,
    level: &'static str,
    notes: Vec<String>,
}

fn gate(conn: &Connection, input: &TaskInput, entry_id: &str) -> AppResult<Result<Passed, Suppressed>> {
    let entry = match catalog::get_entry(conn, entry_id) {
        Ok(e) => e,
        Err(_) => return Ok(Err(suppressed(entry_id, "not-in-catalog", "Bu araç katalogda yok."))),
    };
    if entry.removed_at.is_some() {
        return Ok(Err(suppressed(entry_id, "removed-from-catalog", "Bu araç katalogdan kaldırılmış.")));
    }
    if !entry.enabled {
        return Ok(Err(suppressed(entry_id, "disabled-in-catalog", "Bu kayıt kataloğda etkin değil.")));
    }
    let compat = catalog::compatibility(&entry, &input.platform_target, &input.agent_surface);
    if compat == "unsupported" {
        return Ok(Err(suppressed(entry_id, "unavailable", "Bu platform ve ajan yüzeyinde desteklenmiyor.")));
    }
    let mut notes = Vec::new();
    let known = matches!(compat.as_str(), "vendor-declared" | "independently-tested");
    let level = if entry.execution_policy == "blocked-until-scoped-authorization" {
        notes.push("Kapsamlı yazılı yetki olmadan çalıştırılamaz; bu uygulama bunu hiçbir zaman otomatik çalıştırmaz.".to_string());
        "informational"
    } else if known {
        "suggest"
    } else {
        notes.push(
            "Bu platform ve ajan birleşiminde uyumluluk bilinmiyor; yalnızca bilgi olarak gösterilir, etkinleştirilemez.".to_string(),
        );
        "informational"
    };

    if let Some(pref) = projects::stored_preference(conn, &input.project_alias, entry_id, &input.agent_surface, &input.platform_target)? {
        match pref.decision.as_str() {
            "accepted" => {
                // An earlier approval only covers the functionality it was given for. If capabilities changed
                // materially since, it is not silently reused: the user is asked again.
                if pref.capability_fingerprint == entry.capability_fingerprint {
                    return Ok(Err(suppressed(entry_id, "already-enabled", "Bu proje için zaten etkinleştirmeyi seçtiniz.")));
                }
                notes.push("Daha önce etkinleştirmeyi seçmiştiniz, ancak aracın yetenekleri veya uyumluluğu o zamandan beri önemli ölçüde değişti; kararınız yeniden incelenmeli.".to_string());
            }
            "disabled" => return Ok(Err(suppressed(entry_id, "disabled-by-user", "Bu proje için kapattınız."))),
            "rejected" => {
                // A new version alone never re-prompts. Only a material change in what the tool can do does.
                if pref.capability_fingerprint == entry.capability_fingerprint {
                    return Ok(Err(suppressed(
                        entry_id,
                        "rejected",
                        "Daha önce reddettiniz; sürüm değişikliği yeniden sormak için yeterli değil.",
                    )));
                }
                notes.push("Önceki ret kararınızdan sonra aracın yetenekleri veya uyumluluğu önemli ölçüde değişti.".to_string());
            }
            "postponed" => {
                let due = pref.remind_at.as_deref().map(|r| r <= now_utc().as_str()).unwrap_or(true);
                if !due {
                    return Ok(Err(suppressed(entry_id, "postponed", "Ertelediniz; hatırlatma zamanı henüz gelmedi.")));
                }
                notes.push("Ertelediğiniz hatırlatma zamanı geldi.".to_string());
            }
            _ => {}
        }
    }

    if let Some(hint) = input.availability.iter().find(|h| h.entry_id == entry_id) {
        match (hint.scope.as_str(), hint.enabled) {
            ("project", true) => return Ok(Err(suppressed(entry_id, "already-installed-project", "Bu projede zaten kurulu ve açık görünüyor."))),
            ("global", true) => {
                return Ok(Err(suppressed(
                    entry_id,
                    "already-installed-global",
                    "Genel olarak kurulu ve açık görünüyor: tüm projelerde yüklü olabilir. Proje bazlı kapatma desteği doğrulanmadı; bu uygulama bunu sessizce 'düzeltmez'.",
                )))
            }
            (_, false) => notes.push("Kurulu ama kapalı görünüyor.".to_string()),
            _ => {}
        }
    }
    Ok(Ok(Passed { entry, level, notes }))
}

/// All log reads and writes are scoped by project: the same task id used in two projects is two
/// independent tasks (RT-1B-01).
fn already_shown(conn: &Connection, project_id: &str, task_id: &str, entry_id: &str) -> AppResult<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM suggestion_log WHERE project_id = ?1 AND task_id = ?2 AND entry_id = ?3",
            rusqlite::params![project_id, task_id, entry_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn shown_count(conn: &Connection, project_id: &str, task_id: &str, phase: Option<&str>) -> AppResult<usize> {
    let n: i64 = match phase {
        Some(p) => conn.query_row(
            "SELECT count(*) FROM suggestion_log WHERE project_id = ?1 AND task_id = ?2 AND phase = ?3",
            rusqlite::params![project_id, task_id, p],
            |r| r.get(0),
        )?,
        None => conn.query_row(
            "SELECT count(*) FROM suggestion_log WHERE project_id = ?1 AND task_id = ?2",
            rusqlite::params![project_id, task_id],
            |r| r.get(0),
        )?,
    };
    Ok(n as usize)
}

/// Retention policy: the most recent `MAX_LOG_PER_PROJECT` suggestion records of EACH project.
pub const MAX_LOG_PER_PROJECT: i64 = 500;

/// Keeps the log small per project. `rowid` is allocated globally, so subtracting from it would count other
/// projects' rows as this project's (Issue #4). Instead this project's own rows are ranked newest-first and
/// everything beyond the allowed count is removed; other projects are never touched.
pub(crate) fn prune_log(conn: &Connection, project_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM suggestion_log WHERE project_id = ?1 AND rowid NOT IN
           (SELECT rowid FROM suggestion_log WHERE project_id = ?1 ORDER BY rowid DESC LIMIT ?2)",
        rusqlite::params![project_id, MAX_LOG_PER_PROJECT],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------------------------

/// The whole evaluation (reads, cap checks and log writes) runs in one IMMEDIATE transaction, so two
/// evaluations of the same project and task, even from different connections, are serialized and the
/// per-task cap and once-per-task rule cannot be raced.
pub fn evaluate(conn: &Connection, input: &TaskInput) -> AppResult<Evaluation> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let result = evaluate_in(&tx, input)?;
    tx.commit()?;
    Ok(result)
}

fn evaluate_in(conn: &Connection, input: &TaskInput) -> AppResult<Evaluation> {
    let (text, files, phase) = validate(input)?;
    let project_id: String = conn
        .query_row("SELECT project_id FROM project_registry WHERE alias = ?1 AND removed_at IS NULL", [&input.project_alias], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::new("PROJECT_NOT_FOUND", "Böyle bir proje kaydı yok."))?;

    let intents = classify(&text, &files);
    let mut hits: Vec<Hit> = Vec::new();
    let mut explained: Vec<Suppressed> = Vec::new();

    if intents.frontend {
        hits.push(Hit {
            entry_id: FRONTEND_DESIGN,
            signals: intents.frontend_signals.clone(),
            rationale: "Görev bir arayüz/web tasarımı işi gibi görünüyor; bu eklenti özgün ve üretim kalitesinde ön yüz kodu yazmaya yöneliktir.",
            impact: "Evet derseniz yalnızca bu proje için tercihiniz kaydedilir. Eklenti kurulmaz; kurulum, Claude Code'da sizin onayınızla ayrı bir adımdır.",
        });
    } else if intents.ui_files > 0 {
        explained.push(suppressed(
            FRONTEND_DESIGN,
            "no-ui-intent",
            "Arayüz dosyaları var, ancak görevin kendisi arayüz/tasarım işi gibi görünmüyor; yalnızca dosya varlığı öneri için yeterli değil.",
        ));
    }
    if intents.browser_test {
        hits.push(Hit {
            entry_id: AGENT_BROWSER,
            signals: vec!["görev ifadesi: tarayıcıda test/doğrulama".to_string()],
            rationale: "Görev bir web uygulamasını tarayıcıda denemeyi veya görsel/uçtan uca test etmeyi içeriyor.",
            impact: "Hiçbir tarayıcı açılmaz ve hiçbir site ziyaret edilmez. Kurulursa tarayıcı çalışma zamanı indirir; site yetkisi, ayrı profil ve çerez/ekran görüntüsü gizliliği ayrıca gerekir.",
        });
    }
    if intents.skill_search {
        hits.push(Hit {
            entry_id: SKILLS_COLLECTION,
            signals: vec!["görev ifadesi: yeniden kullanılabilir yetenek arama".to_string()],
            rationale: "Görev, hazır yeniden kullanılabilir bir yetenek (skill) aramayı içeriyor.",
            impact: "Koleksiyonun tamamı kurulmaz. En fazla 1-3 aday gösterilir; her biri kaynak, sürüm ve lisans açısından ayrıca incelenmelidir.",
        });
    }
    if intents.security_assessment {
        hits.push(Hit {
            entry_id: STRIX,
            signals: vec!["görev ifadesi: aktif sızma testi/güvenlik değerlendirmesi planı".to_string()],
            rationale: "Görev yetkili bir güvenlik değerlendirmesi planlamayı içeriyor. Strix gerçek saldırı denemeleri yapan aktif bir araçtır.",
            impact: "Bu yalnızca bilgidir. Çalıştırmak için hedef, ortam, süre, hız, bütçe ve yazılı izin içeren kapsamlı yetki gerekir; bu uygulama hiçbir testi çalıştırmaz.",
        });
    } else if intents.security_keyword {
        explained.push(suppressed(
            STRIX,
            "security-keyword-only",
            "Güvenlik sözcükleri var, ancak aktif sızma testi planlanmıyor; yalnızca anahtar sözcük öneri için yeterli değil.",
        ));
    }

    let mut recommendations: Vec<Recommendation> = Vec::new();
    let mut suppressed_out = explained;
    let mut shown_now = 0usize;
    let already_in_task = shown_count(conn, &project_id, &input.task_id, None)?;
    let late_in_task = shown_count(conn, &project_id, &input.task_id, Some("late"))?;

    for hit in hits {
        let passed = match gate(conn, input, hit.entry_id)? {
            Ok(p) => p,
            Err(s) => {
                suppressed_out.push(s);
                continue;
            }
        };
        if already_shown(conn, &project_id, &input.task_id, hit.entry_id)? {
            suppressed_out.push(suppressed(hit.entry_id, "already-shown-this-task", "Bu görev için zaten gösterildi; tekrar sorulmaz."));
            continue;
        }
        if phase == "late" && late_in_task >= 1 {
            suppressed_out.push(suppressed(hit.entry_id, "late-limit", "Görev başına en fazla bir geç öneri gösterilir."));
            continue;
        }
        if already_in_task + shown_now >= MAX_PER_TASK || (phase == "late" && shown_now >= 1) {
            suppressed_out.push(suppressed(
                hit.entry_id,
                "per-task-limit",
                "Görev başına en fazla iki öneri gösterilir; gereksiz soruları azaltmak için bu atlandı.",
            ));
            continue;
        }
        let mut notes = passed.notes;
        if passed.entry.kind == "agent_plugin" {
            notes.push("Eklenti genel olarak kurulursa tüm projelerde yüklü olabilir; proje bazlı yalıtım, ajan bunu desteklediği doğrulanmadıkça garanti edilmez.".to_string());
        }
        let skill_candidates = if passed.entry.entry_id == SKILLS_COLLECTION {
            let found = skills::search(conn, &passed.entry.entry_id, &text, skills::MAX_CANDIDATES)?;
            if found.is_empty() {
                notes.push("Yetenek dizini boş veya eşleşme yok; hiçbir yetenek kurulmaz.".to_string());
            }
            found
        } else {
            Vec::new()
        };
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO suggestion_log (project_id, task_id, entry_id, phase, shown_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![project_id, input.task_id, hit.entry_id, phase, now_utc()],
        )?;
        if inserted == 0 {
            // Another evaluation of the same project and task logged it first: never show it twice.
            suppressed_out.push(suppressed(hit.entry_id, "already-shown-this-task", "Bu görev için zaten gösterildi; tekrar sorulmaz."));
            continue;
        }
        shown_now += 1;
        recommendations.push(Recommendation {
            entry_id: passed.entry.entry_id.clone(),
            display_name: passed.entry.display_name.clone(),
            level: passed.level,
            can_enable: passed.level == "suggest",
            rationale: hit.rationale.to_string(),
            impact: hit.impact.to_string(),
            signals: hit.signals,
            notes,
            skill_candidates,
        });
    }
    prune_log(conn, &project_id)?;
    Ok(Evaluation {
        task_id: input.task_id.clone(),
        phase: phase.to_string(),
        recommendations,
        suppressed: suppressed_out,
        disclaimer: DISCLAIMER,
    })
}

// ---------------------------------------------------------------------------------------------
// Synthetic scenarios (shared with the UI so the preview can only ever use labelled examples)
// ---------------------------------------------------------------------------------------------

const SCENARIOS_JSON: &str = include_str!("../../data/recommendation-scenarios.json");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub task_text: String,
    pub evidence_files: Vec<String>,
}

pub fn scenarios() -> AppResult<Vec<Scenario>> {
    let root: serde_json::Value = serde_json::from_str(SCENARIOS_JSON)?;
    let list = root
        .get("scenarios")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| AppError::new("SEED_INVALID", "Senaryo verisi geçersiz."))?;
    let text = |v: &serde_json::Value, k: &str| v.get(k).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    Ok(list
        .iter()
        .map(|s| Scenario {
            id: text(s, "id"),
            title: text(s, "title"),
            task_text: text(s, "taskText"),
            evidence_files: s
                .get("evidenceFiles")
                .and_then(serde_json::Value::as_array)
                .map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn setup() -> Connection {
        let conn = crate::db::open_memory().unwrap();
        catalog::seed_all(&conn).unwrap();
        projects::add_reference(&conn, "proje-bir", "Benim sitem", "").unwrap();
        conn
    }

    fn input(task_id: &str, text: &str, files: &[&str]) -> TaskInput {
        TaskInput {
            project_alias: "proje-bir".into(),
            task_id: task_id.into(),
            task_text: text.into(),
            evidence_files: files.iter().map(|f| f.to_string()).collect(),
            agent_surface: "claude-code-terminal".into(),
            platform_target: "windows-x64".into(),
            phase: None,
            availability: vec![],
        }
    }

    fn levels(e: &Evaluation) -> BTreeMap<String, &'static str> {
        e.recommendations.iter().map(|r| (r.entry_id.clone(), r.level)).collect()
    }

    fn reason(e: &Evaluation, entry: &str) -> Option<&'static str> {
        e.suppressed.iter().find(|s| s.entry_id == entry).map(|s| s.reason_code)
    }

    #[test]
    fn every_synthetic_scenario_produces_exactly_the_expected_recommendations() {
        let conn = setup();
        let root: serde_json::Value = serde_json::from_str(SCENARIOS_JSON).unwrap();
        let list = root["scenarios"].as_array().unwrap();
        assert!(list.len() >= 10);
        for s in list {
            let files: Vec<&str> = s["evidenceFiles"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
            let id = s["id"].as_str().unwrap();
            let eval = evaluate(&conn, &input(id, s["taskText"].as_str().unwrap(), &files)).unwrap();
            let expected: BTreeMap<String, String> =
                s["expect"]["entries"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect();
            let got: BTreeMap<String, String> = levels(&eval).into_iter().map(|(k, v)| (k, v.to_string())).collect();
            assert_eq!(got, expected, "scenario {id}: {:?}", eval.suppressed);
            if let Some(sup) = s["expect"]["suppressed"].as_object() {
                for (entry, code) in sup {
                    assert_eq!(reason(&eval, entry), Some(code.as_str().unwrap()), "scenario {id}");
                }
            }
        }
    }

    #[test]
    fn scenarios_are_exposed_for_the_ui_without_expectations() {
        let list = scenarios().unwrap();
        assert!(list.len() >= 10 && list.iter().all(|s| !s.id.is_empty() && !s.task_text.is_empty()));
    }

    #[test]
    fn whole_token_matching_avoids_substring_false_positives() {
        for text in ["Improve access control and build the pipeline", "Update the guide", "Fix classic bug in busy loop"] {
            assert!(!classify(text, &[]).frontend, "{text}");
        }
        assert!(classify("Fix the CSS grid", &[]).frontend);
        assert!(classify("Tasarımı iyileştir: arayüz", &[]).frontend);
        assert!(classify("KULLANICI ARAYÜZÜ'nü yeniden düzenle", &[]).frontend);
    }

    #[test]
    fn css_files_alone_never_trigger_frontend_design_but_tsx_ui_task_without_css_does() {
        let conn = setup();
        let backend =
            evaluate(&conn, &input("t1", "Optimize the database query and fix the migration", &["a.css", "b.scss", "c.html", "d.tsx"]))
                .unwrap();
        assert!(backend.recommendations.is_empty());
        assert_eq!(reason(&backend, FRONTEND_DESIGN), Some("no-ui-intent"));
        let ui = evaluate(&conn, &input("t2", "Design a React dashboard card component", &["Card.tsx"])).unwrap();
        assert_eq!(levels(&ui).get(FRONTEND_DESIGN), Some(&"suggest"));
        assert!(ui.recommendations[0].signals.iter().any(|s| s.contains("react")));
        // A weak intent plus a UI file is enough; a weak intent without any UI evidence is not.
        assert!(!evaluate(&conn, &input("t3", "Improve the design", &[]))
            .unwrap()
            .recommendations
            .iter()
            .any(|r| r.entry_id == FRONTEND_DESIGN));
        assert!(evaluate(&conn, &input("t4", "Improve the design", &["Card.tsx"]))
            .unwrap()
            .recommendations
            .iter()
            .any(|r| r.entry_id == FRONTEND_DESIGN));
    }

    #[test]
    fn each_tool_is_suggested_once_per_task_and_the_cap_limits_prompts() {
        let conn = setup();
        let text = "Design a React component, test it in the browser, and find a reusable skill for charts";
        let first = evaluate(&conn, &input("task-1", text, &["Chart.tsx"])).unwrap();
        assert_eq!(first.recommendations.len(), MAX_PER_TASK);
        assert!(first.suppressed.iter().any(|s| s.reason_code == "per-task-limit"));
        let again = evaluate(&conn, &input("task-1", text, &["Chart.tsx"])).unwrap();
        assert!(again.recommendations.is_empty(), "nothing is asked twice for the same task: {:?}", again.suppressed);
        assert!(again.suppressed.iter().all(|s| matches!(s.reason_code, "already-shown-this-task" | "per-task-limit")));
        // A new task is a new decision point.
        assert_eq!(evaluate(&conn, &input("task-2", text, &["Chart.tsx"])).unwrap().recommendations.len(), MAX_PER_TASK);
    }

    #[test]
    fn at_most_one_late_suggestion_per_task() {
        let conn = setup();
        let mut start = input("task-9", "Fix the bug", &[]);
        assert!(evaluate(&conn, &start).unwrap().recommendations.is_empty());
        start.phase = Some("late".into());
        start.task_text = "Now design the React button component and test it in the browser".into();
        let late = evaluate(&conn, &start).unwrap();
        assert_eq!(late.recommendations.len(), 1);
        let late_again = evaluate(&conn, &input_late("task-9", "Also add another React layout and browser test")).unwrap();
        assert!(late_again.recommendations.is_empty());
        assert!(late_again.suppressed.iter().any(|s| matches!(s.reason_code, "late-limit" | "already-shown-this-task")));
    }

    fn input_late(task_id: &str, text: &str) -> TaskInput {
        let mut i = input(task_id, text, &[]);
        i.phase = Some("late".into());
        i
    }

    #[test]
    fn rejected_stays_rejected_across_version_bumps_until_capabilities_change_or_user_resets() {
        let conn = setup();
        projects::set_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "rejected", None).unwrap();
        let text = "Design a React component";
        assert_eq!(reason(&evaluate(&conn, &input("a1", text, &[])).unwrap(), FRONTEND_DESIGN), Some("rejected"));
        // Version / commit bump only: still silent, and the decision is only flagged informationally.
        conn.execute("UPDATE catalog_entries SET revision = 'v2.0.0@123456789abc' WHERE entry_id = ?1", [FRONTEND_DESIGN]).unwrap();
        assert_eq!(reason(&evaluate(&conn, &input("a2", text, &[])).unwrap(), FRONTEND_DESIGN), Some("rejected"));
        // Material capability change: may be suggested again, and says why.
        conn.execute(
            "UPDATE catalog_entries SET capability_fingerprint = ?1 WHERE entry_id = ?2",
            rusqlite::params!["e".repeat(64), FRONTEND_DESIGN],
        )
        .unwrap();
        let again = evaluate(&conn, &input("a3", text, &[])).unwrap();
        assert_eq!(levels(&again).get(FRONTEND_DESIGN), Some(&"suggest"));
        assert!(again.recommendations[0].notes.iter().any(|n| n.contains("önemli ölçüde değişti")));
        // Explicit reset also allows it again.
        let conn = setup();
        projects::set_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "rejected", None).unwrap();
        projects::reset_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64").unwrap();
        assert_eq!(levels(&evaluate(&conn, &input("b1", text, &[])).unwrap()).get(FRONTEND_DESIGN), Some(&"suggest"));
    }

    #[test]
    fn rejection_on_one_platform_or_surface_does_not_silence_another() {
        let conn = setup();
        projects::set_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "macos-arm64", "rejected", None).unwrap();
        let eval = evaluate(&conn, &input("c1", "Design a React component", &[])).unwrap();
        assert_eq!(levels(&eval).get(FRONTEND_DESIGN), Some(&"suggest"), "the Windows decision is independent");
    }

    #[test]
    fn postponed_is_silent_until_its_reminder_and_accepted_or_disabled_stay_silent() {
        let conn = setup();
        let text = "Design a React component";
        projects::set_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "postponed", Some(7)).unwrap();
        assert_eq!(reason(&evaluate(&conn, &input("p1", text, &[])).unwrap(), FRONTEND_DESIGN), Some("postponed"));
        conn.execute("UPDATE project_preferences SET remind_at = '2000-01-01T00:00:00Z' WHERE entry_id = ?1", [FRONTEND_DESIGN]).unwrap();
        let due = evaluate(&conn, &input("p2", text, &[])).unwrap();
        assert_eq!(levels(&due).get(FRONTEND_DESIGN), Some(&"suggest"));
        assert!(due.recommendations[0].notes.iter().any(|n| n.contains("hatırlatma zamanı geldi")));
        projects::set_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "accepted", None).unwrap();
        assert_eq!(reason(&evaluate(&conn, &input("p3", text, &[])).unwrap(), FRONTEND_DESIGN), Some("already-enabled"));
        projects::set_preference(&conn, "proje-bir", FRONTEND_DESIGN, "claude-code-terminal", "windows-x64", "disabled", None).unwrap();
        assert_eq!(reason(&evaluate(&conn, &input("p4", text, &[])).unwrap(), FRONTEND_DESIGN), Some("disabled-by-user"));
    }

    #[test]
    fn already_installed_hints_silence_the_prompt_and_global_scope_is_disclosed_not_hidden() {
        let conn = setup();
        let text = "Design a React component";
        let mut project_level = input("h1", text, &[]);
        project_level.availability = vec![Availability { entry_id: FRONTEND_DESIGN.into(), scope: "project".into(), enabled: true }];
        assert_eq!(reason(&evaluate(&conn, &project_level).unwrap(), FRONTEND_DESIGN), Some("already-installed-project"));
        let mut global = input("h2", text, &[]);
        global.availability = vec![Availability { entry_id: FRONTEND_DESIGN.into(), scope: "global".into(), enabled: true }];
        let eval = evaluate(&conn, &global).unwrap();
        let why = eval.suppressed.iter().find(|s| s.entry_id == FRONTEND_DESIGN).unwrap();
        assert_eq!(why.reason_code, "already-installed-global");
        assert!(why.reason.contains("tüm projelerde") && why.reason.contains("doğrulanmadı"), "{}", why.reason);
        // Installed but switched off: the suggestion is still allowed, with a note.
        let mut off = input("h3", text, &[]);
        off.availability = vec![Availability { entry_id: FRONTEND_DESIGN.into(), scope: "global".into(), enabled: false }];
        let eval = evaluate(&conn, &off).unwrap();
        assert!(eval.recommendations[0].notes.iter().any(|n| n.contains("kapalı")));
        // None of this wrote a decision.
        assert_eq!(
            conn.query_row::<i64, _, _>("SELECT count(*) FROM project_preferences WHERE entry_id = ?1", [FRONTEND_DESIGN], |r| r.get(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn unknown_compatibility_is_informational_and_cannot_be_enabled_unsupported_is_unavailable() {
        let conn = setup();
        let mut ide = input("u1", "Design a React component", &[]);
        ide.agent_surface = "antigravity-ide".into();
        let eval = evaluate(&conn, &ide).unwrap();
        let r = &eval.recommendations[0];
        assert_eq!((r.level, r.can_enable), ("informational", false));
        assert!(r.notes.iter().any(|n| n.contains("uyumluluk bilinmiyor")));
        conn.execute(
            "UPDATE catalog_entries SET record_json = json_set(record_json, '$.targetEvidence[0].compatibilityState', 'unsupported') WHERE entry_id = ?1",
            [FRONTEND_DESIGN],
        )
        .unwrap();
        let mut mac = input("u2", "Design a React component", &[]);
        mac.platform_target = "macos-arm64".into();
        assert_eq!(reason(&evaluate(&conn, &mac).unwrap(), FRONTEND_DESIGN), Some("unavailable"));
    }

    #[test]
    fn disabled_or_removed_catalog_entries_are_never_suggested() {
        let conn = setup();
        catalog::set_enabled(&conn, FRONTEND_DESIGN, false).unwrap();
        assert_eq!(
            reason(&evaluate(&conn, &input("d1", "Design a React component", &[])).unwrap(), FRONTEND_DESIGN),
            Some("disabled-in-catalog")
        );
        catalog::remove_entry(&conn, AGENT_BROWSER).unwrap();
        assert_eq!(
            reason(&evaluate(&conn, &input("d2", "Test the web app in a browser", &[])).unwrap(), AGENT_BROWSER),
            Some("removed-from-catalog")
        );
    }

    #[test]
    fn strix_is_only_ever_informational_and_never_for_mere_security_words() {
        let conn = setup();
        let plan = evaluate(&conn, &input("s1", "Plan an authorized penetration test of our staging site", &[])).unwrap();
        let r = plan.recommendations.iter().find(|r| r.entry_id == STRIX).unwrap();
        assert_eq!((r.level, r.can_enable), ("informational", false));
        assert!(r.notes.iter().any(|n| n.contains("yazılı yetki")));
        for text in
            ["Review the security of the login code", "Fix the CSRF vulnerability", "Add security headers and fix XSS", "Do a threat model"]
        {
            let e = evaluate(&conn, &input(&format!("s-{}", text.len()), text, &[])).unwrap();
            assert!(!e.recommendations.iter().any(|r| r.entry_id == STRIX), "{text}");
        }
        // Strix stays silent while the user has switched its catalog entry off.
        let conn = setup();
        catalog::set_enabled(&conn, STRIX, false).unwrap();
        let off = evaluate(&conn, &input("s2", "Plan a penetration test", &[])).unwrap();
        assert_eq!(reason(&off, STRIX), Some("disabled-in-catalog"));
    }

    #[test]
    fn ui_and_browser_words_do_not_cross_trigger_other_tools() {
        let conn = setup();
        let e = evaluate(&conn, &input("x1", "Read the docs website in your browser and summarise", &[])).unwrap();
        assert!(!e.recommendations.iter().any(|r| r.entry_id == AGENT_BROWSER), "no test intent");
        let e = evaluate(&conn, &input("x2", "End-to-end test of the payment service", &[])).unwrap();
        assert!(!e.recommendations.iter().any(|r| r.entry_id == AGENT_BROWSER), "no web/browser context");
        let e = evaluate(&conn, &input("x3", "End-to-end test of the web checkout in a browser", &[])).unwrap();
        assert!(e.recommendations.iter().any(|r| r.entry_id == AGENT_BROWSER));
    }

    #[test]
    fn skill_suggestion_returns_at_most_three_provenance_stamped_candidates() {
        let conn = setup();
        let sha = "a".repeat(40);
        let recs: Vec<skills::SkillRecord> = (0..2_500)
            .map(|i| skills::SkillRecord {
                skill_id: format!("pdf-skill-{i}"),
                name: format!("PDF skill {i}"),
                description: "process pdf reports".into(),
                tags: vec!["pdf".into()],
                source_sha: sha.clone(),
                license_spdx: Some("MIT".into()),
            })
            .collect();
        skills::import(&conn, SKILLS_COLLECTION, &recs).unwrap();
        let eval = evaluate(&conn, &input("k1", "Find a reusable skill to process pdf reports", &[])).unwrap();
        let r = eval.recommendations.iter().find(|r| r.entry_id == SKILLS_COLLECTION).unwrap();
        assert_eq!(r.skill_candidates.len(), skills::MAX_CANDIDATES);
        assert!(r.skill_candidates.iter().all(|c| c.source_sha == sha && c.license_spdx.as_deref() == Some("MIT")));
        assert!(serde_json::to_string(&eval).unwrap().len() < 6_000, "the whole answer stays small");
        // Empty index: still a bounded, honest answer.
        let conn = setup();
        let empty = evaluate(&conn, &input("k2", "Find a reusable skill to process pdf reports", &[])).unwrap();
        assert!(empty.recommendations[0].skill_candidates.is_empty());
        assert!(empty.recommendations[0].notes.iter().any(|n| n.contains("dizini boş")));
    }

    #[test]
    fn invalid_input_is_rejected_and_oversized_input_is_bounded() {
        let conn = setup();
        let mut bad = input("ok", "x", &[]);
        bad.task_id = "has space".into();
        assert_eq!(evaluate(&conn, &bad).unwrap_err().code, "TASK_ID_INVALID");
        bad = input("", "x", &[]);
        assert_eq!(evaluate(&conn, &bad).unwrap_err().code, "TASK_ID_INVALID");
        bad = input("ok", "x", &[]);
        bad.agent_surface = "vscode".into();
        assert_eq!(evaluate(&conn, &bad).unwrap_err().code, "SURFACE_INVALID");
        bad = input("ok", "x", &[]);
        bad.platform_target = "linux".into();
        assert_eq!(evaluate(&conn, &bad).unwrap_err().code, "PLATFORM_INVALID");
        bad = input("ok", "x", &[]);
        bad.phase = Some("whenever".into());
        assert_eq!(evaluate(&conn, &bad).unwrap_err().code, "PHASE_INVALID");
        bad = input("ok", "x", &[]);
        bad.project_alias = "yok".into();
        assert_eq!(evaluate(&conn, &bad).unwrap_err().code, "PROJECT_NOT_FOUND");
        let huge = format!("{} React", "word ".repeat(100_000));
        let files: Vec<String> = (0..5_000).map(|i| format!("f{i}.css")).collect();
        let started = std::time::Instant::now();
        let mut big = input("big", "", &[]);
        big.task_text = huge;
        big.evidence_files = files;
        let eval = evaluate(&conn, &big).unwrap();
        assert!(started.elapsed().as_secs() < 5);
        assert!(eval.recommendations.is_empty(), "the keyword sits beyond the 2000-character bound, so it is ignored");
    }

    #[test]
    fn evaluation_never_changes_installation_state_or_writes_decisions() {
        let conn = setup();
        let before: (i64, i64) = (
            conn.query_row("SELECT count(*) FROM project_preferences", [], |r| r.get(0)).unwrap(),
            conn.query_row(
                "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        );
        for (i, s) in scenarios().unwrap().iter().enumerate() {
            let files: Vec<&str> = s.evidence_files.iter().map(String::as_str).collect();
            evaluate(&conn, &input(&format!("n{i}"), &s.task_text, &files)).unwrap();
        }
        let after: (i64, i64) = (
            conn.query_row("SELECT count(*) FROM project_preferences", [], |r| r.get(0)).unwrap(),
            conn.query_row(
                "SELECT count(*) FROM catalog_entries WHERE install_state <> 'not_managed' OR verification_state <> 'unknown'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        );
        assert_eq!(before, after);
        assert_eq!(after.1, 0);
    }
}
