//! Phase 3D-2: the in-app confirmation book.

use super::*;
use chrono::TimeZone;
use std::sync::{Arc, Barrier};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2027, 1, 1, 12, 0, 0).unwrap()
}

fn binding(tool: &str, preview: &str) -> Binding {
    Binding {
        action: Action::Install,
        tool_id: tool.to_string(),
        version: "1.0.0".to_string(),
        platform_target: "windows-x64".to_string(),
        computer_id: "computer-1".to_string(),
        recipe_digest: "a".repeat(64),
        artifact_sha256: "b".repeat(64),
        effects_digest: "c".repeat(64),
        plan_id: "d".repeat(64),
        preview_digest: preview.to_string(),
        from_version: None,
    }
}

#[test]
fn a_preview_confirmed_once_gives_exactly_one_grant_bound_to_what_was_previewed() {
    let book = ConsentBook::new();
    let b = binding("synthetic-a", &"1".repeat(64));
    let ticket = book.open(b.clone(), t0());
    assert!(is_offer_id(&ticket.offer_id));
    assert_eq!(ticket.expires_at, t0() + Duration::seconds(OFFER_TTL_SECONDS));
    let grant = book.confirm(&ticket.offer_id, &"1".repeat(64), true, t0() + Duration::seconds(5)).unwrap();
    assert_eq!(grant.binding(), &b);
    assert_eq!(grant.actor().kind, "in_app_confirmation");
    assert_eq!(book.live_count(), 0, "the offer left the book when it was confirmed");
}

#[test]
fn a_confirmation_replay_is_refused_with_its_true_reason() {
    let book = ConsentBook::new();
    let t = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    book.confirm(&t.offer_id, &"1".repeat(64), true, t0()).unwrap();
    assert_eq!(book.confirm(&t.offer_id, &"1".repeat(64), true, t0()).unwrap_err(), ConsentError::AlreadyUsed);
    assert_eq!(book.cancel(&t.offer_id, t0()).unwrap_err(), ConsentError::AlreadyUsed);
}

#[test]
fn a_wrong_echo_or_a_missing_acknowledgement_burns_the_offer() {
    let book = ConsentBook::new();
    let t = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    assert_eq!(book.confirm(&t.offer_id, &"2".repeat(64), true, t0()).unwrap_err(), ConsentError::PreviewChanged);
    // the right echo afterwards no longer helps: the user has to preview again
    assert!(book.confirm(&t.offer_id, &"1".repeat(64), true, t0()).is_err());
    let t2 = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    assert_eq!(book.confirm(&t2.offer_id, &"1".repeat(64), false, t0()).unwrap_err(), ConsentError::NotAcknowledged);
    assert!(book.confirm(&t2.offer_id, &"1".repeat(64), true, t0()).is_err());
}

#[test]
fn an_expired_offer_is_refused_and_a_cancelled_one_can_never_be_confirmed() {
    let book = ConsentBook::new();
    let t = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    let late = t0() + Duration::seconds(OFFER_TTL_SECONDS);
    assert_eq!(book.confirm(&t.offer_id, &"1".repeat(64), true, late).unwrap_err(), ConsentError::Expired);
    let t = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    book.cancel(&t.offer_id, t0()).unwrap();
    assert_eq!(book.confirm(&t.offer_id, &"1".repeat(64), true, t0()).unwrap_err(), ConsentError::Cancelled);
    // the last moment before expiry still works
    let t = book.open(binding("synthetic-b", &"1".repeat(64)), t0());
    assert!(book.confirm(&t.offer_id, &"1".repeat(64), true, t0() + Duration::seconds(OFFER_TTL_SECONDS - 1)).is_ok());
}

#[test]
fn a_newer_preview_of_the_same_tool_replaces_the_older_one_and_other_tools_are_untouched() {
    let book = ConsentBook::new();
    let old = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    let other = book.open(binding("synthetic-b", &"3".repeat(64)), t0());
    let new = book.open(binding("synthetic-a", &"2".repeat(64)), t0());
    assert_eq!(book.confirm(&old.offer_id, &"1".repeat(64), true, t0()).unwrap_err(), ConsentError::Superseded);
    assert!(book.confirm(&new.offer_id, &"2".repeat(64), true, t0()).is_ok());
    assert!(book.confirm(&other.offer_id, &"3".repeat(64), true, t0()).is_ok());
}

#[test]
fn forged_identifiers_are_refused_before_the_book_is_consulted() {
    let book = ConsentBook::new();
    let real = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    for bad in [
        "",
        "x",
        &"A".repeat(64),                // upper case
        &"g".repeat(64),                // not hex
        &real.offer_id[..63],           // too short
        &format!("{}0", real.offer_id), // too long
        "..\\..\\windows\\system32\\cmd.exe",
        "C:\\Users\\someone\\file",
        &format!("{} OR 1=1", real.offer_id),
    ] {
        assert_eq!(book.confirm(bad, &"1".repeat(64), true, t0()).unwrap_err(), ConsentError::BadId, "{bad:?}");
    }
    // a well-formed but unknown identifier is merely unknown
    assert_eq!(book.confirm(&"0".repeat(64), &"1".repeat(64), true, t0()).unwrap_err(), ConsentError::Unknown);
    // none of that touched the real offer
    assert_eq!(book.live_count(), 1);
    assert!(book.confirm(&real.offer_id, &"1".repeat(64), true, t0()).is_ok());
}

#[test]
fn many_racing_confirmations_of_one_offer_produce_exactly_one_grant() {
    let book = Arc::new(ConsentBook::new());
    let t = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    let barrier = Arc::new(Barrier::new(16));
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let (book, barrier, id) = (book.clone(), barrier.clone(), t.offer_id.clone());
            std::thread::spawn(move || {
                barrier.wait();
                book.confirm(&id, &"1".repeat(64), true, t0()).map(|g| g.grant_id().to_string())
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
    assert!(results.iter().filter_map(|r| r.as_ref().err()).all(|e| *e == ConsentError::AlreadyUsed));
}

#[test]
fn identifiers_are_long_unique_and_the_book_is_bounded() {
    let book = ConsentBook::new();
    let mut seen = std::collections::HashSet::new();
    for i in 0..40 {
        let t = book.open(binding(&format!("synthetic-{i}"), &"1".repeat(64)), t0());
        assert!(seen.insert(t.offer_id));
    }
    assert!(book.live_count() <= 16);
}

#[test]
fn live_offers_are_visible_per_tool_until_they_end() {
    let book = ConsentBook::new();
    assert!(book.live_for("synthetic-a", t0()).is_none());
    let t = book.open(binding("synthetic-a", &"1".repeat(64)), t0());
    assert_eq!(book.live_for("synthetic-a", t0()).unwrap().0, Action::Install);
    assert!(book.live_for("synthetic-b", t0()).is_none());
    assert!(book.live_for("synthetic-a", t0() + Duration::seconds(OFFER_TTL_SECONDS)).is_none());
    assert_eq!(
        book.confirm(&t.offer_id, &"1".repeat(64), true, t0() + Duration::seconds(OFFER_TTL_SECONDS)).unwrap_err(),
        ConsentError::Expired
    );
}

#[test]
fn a_grant_cannot_be_copied_serialised_or_built_by_other_modules() {
    let src = include_str!("consent.rs").split("#[cfg(test)]\n#[path").next().unwrap();
    let at = src.find("pub struct ConsentGrant").unwrap();
    let derive = src[..at].lines().last().unwrap_or_default().trim().to_string();
    assert_eq!(derive, "#[derive(Debug)]", "the grant derives nothing but Debug");
    // the fields are private, so no other module can construct it with a struct literal
    assert!(!src.contains("pub grant_id") && !src.contains("pub granted_at"));
    // nothing in this module reaches the file system, a process or the network
    for needle in ["std::fs", "std::process", "std::net", "Command::new", "reqwest"] {
        assert!(!src.contains(needle), "{needle}");
    }
}
