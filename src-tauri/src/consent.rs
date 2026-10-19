//! Phase 3D-2: the in-application confirmation ("offer" -> "grant"), checked in Rust.
//!
//! The first useful installation experience needs a normal, visible confirmation, not a hardware credential. This module is that
//! confirmation and nothing more:
//!
//! * Rust makes an **offer** when the user opens an installation preview. The offer is bound to the exact preview content (a digest
//!   over tool, version, platform, artifact digest, filesystem effects, notices, plan and computer), is short-lived and lives in memory
//!   only: closing the application discards every offer.
//! * The user **confirms** that very preview with an explicit affirmative action. Rust accepts the confirmation only for a live offer
//!   whose preview digest the interface echoes back unchanged. The offer is *removed* from the book at that moment, so a second
//!   confirmation of the same offer (a replay, or two racing requests) finds nothing; any refused attempt on a live offer burns it too.
//! * What confirmation produces is a [`ConsentGrant`]: a value that cannot be cloned, serialised or built outside this module, and that
//!   the adapter consumes **by value** after it has taken the tool's lock and before its first change.
//!
//! # What this is not
//!
//! An in-app confirmation is **not authentication** and **not cryptographic proof that a person was present**. A random offer
//! identifier is an unguessable handle, not a secret the user holds; the interface that shows the preview is the interface that sends
//! the confirmation; and any program running as the same Windows account may be able to drive that interface or the application's IPC.
//! What the mechanism does give: nothing is applied without an explicit confirmation of exactly the previewed content, a stale,
//! cancelled, replayed or altered confirmation is refused, and every outcome is audited. Higher-risk operations keep their own, separate
//! future gates (see `docs/phase-3d2/MVP-CONSENT-MODEL.md`); this module never widens what the adapter and the recipe gates allow.

use crate::approval::{Action, Actor};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
};

/// How long a preview may wait for its confirmation.
pub const OFFER_TTL_SECONDS: i64 = 120;
/// How long a confirmed grant may take to be used by the adapter (preflight, lock, staging, switch, receipt).
pub const GRANT_TTL_SECONDS: i64 = 120;
const MAX_LIVE_OFFERS: usize = 16;
const MAX_REMEMBERED: usize = 128;

/// Everything a confirmation is bound to. The adapter re-derives each field from the recipe and its own state and refuses any difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub action: Action,
    pub tool_id: String,
    pub version: String,
    pub platform_target: String,
    pub computer_id: String,
    pub recipe_digest: String,
    pub artifact_sha256: String,
    pub effects_digest: String,
    pub plan_id: String,
    /// Digest of the complete preview the user saw (effects, notices, source, rollback text). A different preview is a different consent.
    pub preview_digest: String,
    /// For an update: the installed version being replaced. `None` for install and removal.
    pub from_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentError {
    /// The identifier is not shaped like an offer identifier.
    BadId,
    /// No such offer was ever made (or it was forgotten long ago).
    Unknown,
    /// Already confirmed once: a replay.
    AlreadyUsed,
    Cancelled,
    Expired,
    /// A newer preview for the same tool replaced it, or the book was full.
    Superseded,
    /// The preview the interface echoed is not the preview Rust holds.
    PreviewChanged,
    /// The explicit affirmative action is missing.
    NotAcknowledged,
}

impl ConsentError {
    pub fn code(self) -> &'static str {
        match self {
            ConsentError::BadId => "consent_bad_identifier",
            ConsentError::Unknown => "consent_unknown",
            ConsentError::AlreadyUsed => "consent_already_used",
            ConsentError::Cancelled => "consent_cancelled",
            ConsentError::Expired => "consent_expired",
            ConsentError::Superseded => "consent_superseded",
            ConsentError::PreviewChanged => "consent_preview_changed",
            ConsentError::NotAcknowledged => "consent_not_acknowledged",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gone {
    Used,
    Cancelled,
    Expired,
    Superseded,
}

struct Offer {
    binding: Binding,
    expires_at: DateTime<Utc>,
}

#[derive(Default)]
struct Inner {
    live: HashMap<String, Offer>,
    /// What happened to offers that are no longer live (bounded; only to give a replay a truthful reason).
    gone: VecDeque<(String, Gone)>,
}

impl Inner {
    fn remember(&mut self, id: String, why: Gone) {
        self.gone.retain(|(i, _)| *i != id);
        self.gone.push_back((id, why));
        while self.gone.len() > MAX_REMEMBERED {
            self.gone.pop_front();
        }
    }

    fn expire(&mut self, now: DateTime<Utc>) {
        let stale: Vec<String> = self.live.iter().filter(|(_, o)| now >= o.expires_at).map(|(i, _)| i.clone()).collect();
        for id in stale {
            self.live.remove(&id);
            self.remember(id, Gone::Expired);
        }
    }
}

/// What the interface gets back when an offer is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub offer_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// The live offers of this process. Memory only, on purpose.
#[derive(Default)]
pub struct ConsentBook {
    inner: Mutex<Inner>,
}

/// 256 bits from the operating system's random source (two version-4 UUIDs), as 64 lower-case hex characters.
fn new_id() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn is_offer_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl ConsentBook {
    pub fn new() -> ConsentBook {
        ConsentBook::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // a poisoned book is still a book: every entry in it is either complete or absent
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Opens an offer for `binding`. A newer offer for the same tool replaces every older one (the preview on screen is the latest).
    pub fn open(&self, binding: Binding, now: DateTime<Utc>) -> Ticket {
        let mut g = self.lock();
        g.expire(now);
        let older: Vec<String> = g.live.iter().filter(|(_, o)| o.binding.tool_id == binding.tool_id).map(|(i, _)| i.clone()).collect();
        for id in older {
            g.live.remove(&id);
            g.remember(id, Gone::Superseded);
        }
        while g.live.len() >= MAX_LIVE_OFFERS {
            let Some(oldest) = g.live.iter().min_by_key(|(_, o)| o.expires_at).map(|(i, _)| i.clone()) else { break };
            g.live.remove(&oldest);
            g.remember(oldest, Gone::Superseded);
        }
        let ticket = Ticket { offer_id: new_id(), created_at: now, expires_at: now + Duration::seconds(OFFER_TTL_SECONDS) };
        g.live.insert(ticket.offer_id.clone(), Offer { binding, expires_at: ticket.expires_at });
        ticket
    }

    fn why_gone(g: &Inner, id: &str) -> ConsentError {
        match g.gone.iter().find(|(i, _)| i == id).map(|(_, w)| *w) {
            Some(Gone::Used) => ConsentError::AlreadyUsed,
            Some(Gone::Cancelled) => ConsentError::Cancelled,
            Some(Gone::Expired) => ConsentError::Expired,
            Some(Gone::Superseded) => ConsentError::Superseded,
            None => ConsentError::Unknown,
        }
    }

    /// The explicit affirmative action. On success the offer is gone and a one-time grant is returned. On ANY refusal of a live
    /// offer the offer is burned as well: whoever sent a wrong echo must preview again.
    pub fn confirm(
        &self,
        offer_id: &str,
        echoed_preview_digest: &str,
        acknowledged: bool,
        now: DateTime<Utc>,
    ) -> Result<ConsentGrant, ConsentError> {
        if !is_offer_id(offer_id) {
            return Err(ConsentError::BadId);
        }
        let mut g = self.lock();
        g.expire(now);
        let Some(offer) = g.live.remove(offer_id) else { return Err(Self::why_gone(&g, offer_id)) };
        if !acknowledged {
            g.remember(offer_id.to_string(), Gone::Cancelled);
            return Err(ConsentError::NotAcknowledged);
        }
        if offer.binding.preview_digest != echoed_preview_digest {
            g.remember(offer_id.to_string(), Gone::Cancelled);
            return Err(ConsentError::PreviewChanged);
        }
        g.remember(offer_id.to_string(), Gone::Used);
        Ok(ConsentGrant {
            grant_id: new_id(),
            binding: offer.binding,
            granted_at: now,
            expires_at: now + Duration::seconds(GRANT_TTL_SECONDS),
        })
    }

    /// The user closed the preview without confirming. A cancelled offer can never be confirmed afterwards.
    pub fn cancel(&self, offer_id: &str, now: DateTime<Utc>) -> Result<Binding, ConsentError> {
        if !is_offer_id(offer_id) {
            return Err(ConsentError::BadId);
        }
        let mut g = self.lock();
        g.expire(now);
        match g.live.remove(offer_id) {
            Some(o) => {
                g.remember(offer_id.to_string(), Gone::Cancelled);
                Ok(o.binding)
            }
            None => Err(Self::why_gone(&g, offer_id)),
        }
    }

    /// The live offer for a tool, if any (what the interface shows as "waiting for your approval").
    pub fn live_for(&self, tool_id: &str, now: DateTime<Utc>) -> Option<(Action, DateTime<Utc>)> {
        let mut g = self.lock();
        g.expire(now);
        g.live.values().find(|o| o.binding.tool_id == tool_id).map(|o| (o.binding.action, o.expires_at))
    }

    #[cfg(test)]
    pub fn live_count(&self) -> usize {
        self.lock().live.len()
    }
}

/// The result of an accepted confirmation. Not `Clone`, not `Serialize`, no public constructor: it exists once and the adapter consumes
/// it by value.
#[derive(Debug)]
pub struct ConsentGrant {
    grant_id: String,
    binding: Binding,
    granted_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl ConsentGrant {
    pub fn grant_id(&self) -> &str {
        &self.grant_id
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    pub fn granted_at(&self) -> DateTime<Utc> {
        self.granted_at
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// The Phase 3A actor for this grant. Only a grant can produce it, so an approval of this kind cannot exist without a confirmation.
    pub fn actor(&self) -> Actor {
        Actor::in_app(&format!("in-app-{}", &self.grant_id[..16]))
    }
}

#[cfg(test)]
impl ConsentGrant {
    /// Test seam: a grant that never went through a book, for tests of the consumer. Never compiled into a release build.
    pub fn for_test(binding: Binding, now: DateTime<Utc>) -> ConsentGrant {
        ConsentGrant { grant_id: new_id(), binding, granted_at: now, expires_at: now + Duration::seconds(GRANT_TTL_SECONDS) }
    }
}

#[cfg(test)]
#[path = "consent_tests.rs"]
mod tests;
