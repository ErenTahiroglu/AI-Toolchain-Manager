# PRODUCT-06 — evidence origin: the contract and what the screen may say

Baseline (verified before any edit): `development` = `origin/development` = `1207dc054f89712d1409ab0b63a4bbe0dbad6c35`, `origin/main` = `68eda3f0407490fabccb517dcf822ab4818fc0bf` (untouched), clean tracked and untracked tree, one worktree, no stash. REAL_INSTALLATION_NO_GO. Issues #14 and #25 are untouched. Nothing here is an independent acceptance claim, and nothing here says "production-ready".

## Why
`store.rs::classify_request` chooses the stored observation behind an accepted wish's status. That observation may be project-specific or computer-wide, and may have been recorded for the requested agent surface or for `local-cli` (a computer-level program lookup that stands in for any surface). The response did not say which, so the screen could not tell a user whether `present_current` meant "found for this project" or "a program of that name exists on this computer". The UI must not infer identity, installation, functionality or per-project activation from a computer-wide name match. PRODUCT-06 makes the origin explicit.

## The Rust contract (additive)
`RequestVsObservation` gains one member, `origin: Option<EvidenceOrigin>` (wire name `origin`, `null` when `None`). Every earlier member is unchanged, in name, type and meaning.

```rust
pub enum ProjectRelation { ProjectSpecific, ComputerWide }      // wire: "project_specific" | "computer_wide"
pub enum SurfaceRelation { ExactSurface, LocalCliFallback }     // wire: "exact_surface" | "local_cli_fallback"
pub struct EvidenceOrigin {                                     // wire (camelCase):
    pub project_relation: ProjectRelation,                      //   projectRelation
    pub surface_relation: SurfaceRelation,                      //   surfaceRelation
    pub observed_at: Option<String>,                            //   observedAt   (string | null)
    pub age_hours: Option<i64>,                                 //   ageHours     (number | null)
}
```

| Member | Comes from | Rule |
|---|---|---|
| `projectRelation` | the selected observation's `project_alias` | `Some(_)` → `project_specific` (the relevance filter already guarantees it equals the preference's alias); `None` → `computer_wide`. The stored `scope` column is **not** used: it does not say how the row was matched. |
| `surfaceRelation` | the selected observation's `agent_surface` against the requested surface | equal → `exact_surface`; otherwise `local_cli_fallback` (the relevance filter admits only the requested surface or `local-cli`). A `local-cli` row for a request that itself asks for `local-cli` is `exact_surface`: nothing is a fallback there. |
| `observedAt` | the selected observation's `observed_at` | kept as stored, only if it parses as RFC 3339; otherwise `null`. Never converted, repaired or defaulted. |
| `ageHours` | the selected observation's `age_hours` (computed when the inventory is read) | only when `observedAt` is valid **and** the age is not negative (a future timestamp gives `null`); otherwise `null`. The stored age is capped at `i32::MAX` for unparsable stamps, so validity is decided by parsing the stamp, not by that sentinel. |
| `origin` as a whole | the one observation `classify_request` selected | `null` **exactly** when no observation was selected: `not_scanned` and `platform_mismatch`. Then `basis` is `null` too. No scope is guessed. |

Every field is derived from the **winner** of the existing selection (see below), never from the candidate set. A historical winner (`previously_*`) is described by its own time and relations; the status already says it is not current, and adding metadata does not make it current.

Not in the response and never rendered: path, `location`, evidence text, limitations, fingerprint contents, version, `scope`, `method`, observation id.

### Selection and statuses did not change (proof)
`classify_request` keeps its relevance filter, platform filter, current/historical split, and `max_by_key((rank(presence), id))`. Its return value gained a fourth tuple member; the first three are produced by the same code paths as before. Evidence, all in the repository:
* The diff of `classify_request` is: the return type (+1 member), `None` → `None, None` in the two no-observation returns, and `, Some(origin_of(o, surface))` appended to the two returns that already had a `Some(scan_ref(o))`. No line of the filter or ranking changed.
* `tests_origin.rs::selection_status_note_and_basis_are_identical_to_the_version_without_an_origin` carries a verbatim copy of the PRODUCT-05 function and compares it with the new one over 8 000 deterministic cases (4 000 random observation sets × two requested surfaces, mixing project aliases, surfaces, platforms, presences, current/historical, ids, valid and invalid stamps): status, note and the selected scan are identical in every case; in every case with a selection, the origin equals what the selected row says; where there is none, origin and basis are both `None`. The generator is asserted to reach every outcome.
* The earlier `tests_history.rs` tests (`.0` of the tuple) pass unchanged.

## What the screen says (Turkish, descriptive, no verdict)
Under the matrix observation ("Son kayıtlı envanter gözlemi") and in each review-queue item that has a stored status, a block **"Kanıtın kaynağı"** stating it is separate from the saved wish and from the application-managed install record. Its lines:

| Wire value | Line |
|---|---|
| `computer_wide` | "Kanıt kapsamı: bilgisayar geneli. Projeden bağımsız bir kayıttır; bu projeye özgü bir kanıt değildir." |
| `project_specific` | "Kanıt kapsamı: bu proje. Gözlem, yalnızca arka uçta kaydedilen proje takma adı kadarıyla bu projeyle ilişkilendirildi; bir ajanın bu aracı kullandığını ya da kimliğinin onaylandığını göstermez." |
| `exact_surface` | "Ajan yüzeyi: istenen yüzey için kaydedilmiş gözlem (o yüzeyin bu aracı yapılandırdığı anlamına gelmez)." |
| `local_cli_fallback` | "Ajan yüzeyi: yedek eşleşme. İstenen yüzey yerine bilgisayardaki program araması (local-cli) kullanıldı; bu, istenen ajan yüzeyinin bu aracı yapılandırdığına kanıt değildir." |
| valid `observedAt` | "Gözlem zamanı: … (envanter okunduğunda yaklaşık N saat önce). Bu, kaydedilmiş bir tarama sonucuna aittir; canlı bir doğrulama değildir." (the age part only with a valid age) |
| `origin` null, unreadable, or an unknown relation | "Kanıtın kaynağı belirlenemedi; bilgisayar geneli mi, projeye özel mi olduğu söylenemez." (+ "Bu durumda seçilmiş bir gözlem yok." when there is no basis scan) |

The label is "Kanıt kapsamı" on purpose: the existing cards already use "Kapsam" for the *install* scope. The status text, note, basis scan and permissions line stay exactly as they were. Ambiguous (two rows for one key), missing-row, unrecognised-status, not-accepted and removed-entry cases render no origin at all. The rest of the PRODUCT-04/05 handling (loading, unreadable, old after a failed refresh) is untouched.

## Deliverable 0 — P05-RT-02, the headline was not a lower bound
`reviewHeadline` said "En az N öğe …" for every unfinished queue. While a refresh runs, the retained list can **shrink** (six cached items can become five), so "at least N" was false. It now says what it knows: **"Eldeki geçici görüntüde N öğe var; okuma sürüyor, sayı artabilir veya azalabilir (sonuç kesin değil)."** (a read is running) or **"… liste eksik veya eski olabilir, sayı artabilir veya azalabilir (sonuç kesin değil)."** (a failed or missing read). The exact sentence "N öğe incelenmeyi bekliyor." and the clean-zero sentence still appear only when both reads are finished and clean. One formatter (`reviewHeadline`) serves the queue and the Dashboard tile. No persistent acknowledgement was added.

## Backward compatibility
The member is additive. The frontend treats a missing `origin` (an older backend), `null`, a non-object, or any value outside the two known relations as "unavailable"; it never guesses. The Inventory page reads `requests` but does not use the new member and is unchanged.
