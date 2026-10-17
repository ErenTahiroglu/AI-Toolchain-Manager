//! Redaction applied before anything is persisted as a diagnostic detail.
//! It is a best-effort filter, not proof that no secret remains; the design rule is that
//! raw command output, file contents and credentials are never passed to it in the first place.

use regex::Regex;
use std::sync::OnceLock;

const POLICY_PLACEHOLDERS: &[(&str, &str)] = &[
    // URL credentials first so the password is gone before other rules run.
    (r"(?i)[a-z][a-z0-9+.\-]*://[^/\s:@]+:[^/\s@]+@", "[url-credentials]@"),
    (r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{5,}", "[jwt]"),
    (r"\bgh[pousr]_[A-Za-z0-9]{20,}", "[token]"),
    (r"\bgithub_pat_[A-Za-z0-9_]{20,}", "[token]"),
    (r"\bsk-[A-Za-z0-9_\-]{16,}", "[token]"),
    (r"\bAKIA[0-9A-Z]{16}\b", "[token]"),
    (r"(?i)bearer\s+[A-Za-z0-9._\-]{8,}", "Bearer [token]"),
    (r"(?i)(api[_\-]?key|token|secret|passw(?:or)?d|pwd)\s*[:=]\s*\S+", "$1=[redacted]"),
    (r#"(?i)\b[a-z]:\\[^\s"'<>|]*"#, "[path]"),
    (r#"\\\\[^\s"'<>|]+"#, "[path]"),
    (r#"(?:/Users|/home|/var|/tmp|/private|/etc|/opt|/root)/[^\s"'<>|]*"#, "[path]"),
    (r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}", "[email]"),
];

fn rules() -> &'static Vec<(Regex, &'static str)> {
    static RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RULES.get_or_init(|| {
        POLICY_PLACEHOLDERS.iter().map(|(pattern, repl)| (Regex::new(pattern).expect("valid redaction pattern"), *repl)).collect()
    })
}

/// Redact, strip control characters and cap the length (in characters).
pub fn sanitize(input: &str, max_chars: usize) -> String {
    let mut text: String = input.chars().map(|c| if c.is_control() && c != ' ' { ' ' } else { c }).collect();
    for (re, repl) in rules() {
        text = re.replace_all(&text, *repl).into_owned();
    }
    let trimmed = text.trim();
    if trimmed.chars().count() > max_chars {
        let cut: String = trimmed.chars().take(max_chars).collect();
        format!("{cut}…")
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // All values below are dummy strings made up for the test.
    #[test]
    fn removes_dummy_secrets_and_paths() {
        let raw = "failed C:\\Users\\dummy-user\\secret\\file.txt token=abc123DUMMY \
                   ghp_DUMMYDUMMYDUMMYDUMMYDUMMY1234 Authorization: Bearer dummy.value.12345 \
                   https://user:pw@example.invalid/x /Users/dummy/project a@b.example";
        let out = sanitize(raw, 1000);
        for leaked in ["dummy-user", "abc123DUMMY", "ghp_DUMMY", "dummy.value.12345", "user:pw", "/Users/dummy", "a@b.example"] {
            assert!(!out.contains(leaked), "leaked {leaked} in {out}");
        }
        assert!(out.contains("[path]"));
    }

    #[test]
    fn caps_length_and_strips_control_characters() {
        let out = sanitize(&format!("a\u{0007}\nb{}", "x".repeat(500)), 20);
        assert!(out.chars().count() <= 21);
        assert!(!out.chars().any(|c| c.is_control()));
    }

    #[test]
    fn harmless_text_is_untouched() {
        assert_eq!(sanitize("Tema değişti: dark", 100), "Tema değişti: dark");
    }
}
