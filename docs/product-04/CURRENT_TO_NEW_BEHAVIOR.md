# PRODUCT-04 — "Son kayıtlı envanter gözlemi" in the Project Tool Workspace: current → new behaviour

Baseline: `development` at `840bfd7f261127acf96e976fde51176370f40880` (PRODUCT-03). `main` stays at `68eda3f0407490fabccb517dcf822ab4818fc0bf`.
Frontend only. **Not** production-ready; real installation remains NO-GO (Issues #14 and #25 open).

## What question the matrix answers now
For one project + one operating system + one agent surface + one tool:

1. **What do I want?** — the recorded wish ("İsteğiniz"), unchanged.
2. **What did the last stored, authorized discovery evidence say?** — new, read-only section **"Son kayıtlı envanter gözlemi"**, in the same row, next to the wish and visibly separate from it and from "Gerçek durum" (the install/verification record).
3. **What is still unknown?** — said explicitly: a name match is not an identity, an observation is not a functional check, a catalog switch is not an agent setting, and a wish is not an installation. Where the backend gave no row, nothing is invented.

## Before → after

| | Before (PRODUCT-03) | After (PRODUCT-04) |
|---|---|---|
| Inventory in the matrix | none (the page asserted it never called `list_inventory`) | one extra typed **read**: `api.listInventory()` → `list_inventory`, loaded with the existing `useLoad` (latest request wins) in `Projects.tsx` and passed to the workspace |
| Wish vs observation | only on the separate "Bilgisayar Envanteri" page | per row, side by side (the section is a third block of the row's grid) |
| Scanning | never from the matrix | **still never**; no scan control exists in the matrix and nothing calls `run_discovery_scan` |
| Re-reads | projects + catalog on refresh/save | projects + catalog + stored inventory on *Listeyi yenile*; projects + inventory after a save/reset. Opening, switching target or filtering re-reads nothing |
| Preference commands | `set_project_preference` / `reset_project_preference` on the row's own buttons | **exactly as before** |
| Backend / schema / capabilities / dependencies | — | **unchanged** |

## Source of truth and the join
`list_inventory` → `discovery::store::inventory(conn)`: `latest_observations`, `last_scan`, `requests` — SQL over the manager's own database; no file, process or network API appears in `store.rs` (grep for `std::fs`, `Command::new`, `reqwest`, `fs::` finds nothing; verified by reading, **not** by running Rust locally).
`Inventory.requests` holds one `RequestVsObservation` per **accepted** preference whose catalog entry and project are not removed. The frontend joins a matrix row to it by **all four** fields: `projectAlias`, `entryId`, `platformTarget`, `agentSurface` (`requestMatches` in `src/observation.ts`). Never by tool id, display name, source name or project name alone; never across projects or platforms. Two rows for one key are contradictory and neither is shown.
The frontend does **not** re-derive a status from `Inventory.observations`; doing so would be a second, weaker classifier. Raw observation rows are not used at all by the matrix.

## What is shown for each state of a row

| Row state | Shown |
|---|---|
| Accepted wish + exactly one matching request | `observedLabel(status)` in bold, the **backend's own note**, "Dayanak tarama: *finish time* · taramaya verilen izinler: …" when `basis` exists, otherwise "Bu durum için bir dayanak tarama kaydı gelmedi." |
| Accepted wish, no matching request in the read | "eşleşen satır yok … Bir gözlem varsayılmaz … Karar bu envanter okumasından sonra kaydedilmiş olabilir." No status label. |
| Wish is not `accepted` (postponed, rejected, disabled, unrecognised, none) | "kabul edilmiş bir isteğiniz yok; … gözlemle karşılaştırma yapılmadı. Bu, aracın bilgisayarda olmadığı ya da hiç taranmadığı anlamına gelmez." — even if an older read still holds a row for that key (the **current wish** decides, not a cached row) |
| Catalog entry removed | "katalogdan kaldırıldı … karşılaştırması yapılmaz ve güncel bir gözlem varsayılmaz" — whatever the inventory holds |
| Inventory pending | "Kayıtlı envanter okunuyor…" (no status) |
| First read failed | error notice with the code and **Yeniden dene**, and per row "gözlem **bilinmiyor** … 'taranmadı' veya 'bulunamadı' anlamına gelmez". Rows that never depended on the inventory (no accepted wish, removed) are unaffected |
| Refresh failed after a good read | the earlier answer stays, marked "⚠ Envanter yenilenemedi; bu, önceki başarılı okumadan ve güncel olmayabilir." (and the same for the "eşleşen satır yok" case) |
| Status string this version does not know | "tanımadığı bir gözlem durumu … Başarı sayılmaz"; no label, nothing concluded |

Every section also carries the sentence *"Bu, son kaydedilen tarama sonucudur; şu anki bir denetim değildir. Bu ekran hiçbir tarama başlatmaz ve bilgisayara bakmaz."* and a collapsed list **"Bu gözlem neyi göstermez?"** (name match ≠ identity; found ≠ installed/working/authenticated/verified; catalog "etkin" ≠ used by the agent; wish ≠ installed; a computer-wide name lookup may count for every project; details and re-scan only on the Bilgisayar Envanteri page).

## Precise meaning of each status (as the Rust classifier defines them)
`classify_request(entry, project, surface, platform, observations)` in `store.rs`. An observation is **relevant** to a wish when it is about the same entry, belongs to this project **or is computer-wide** (`project_alias` is null), and is for the same agent surface **or a computer-level `local-cli` lookup**. Among relevant observations for the **same platform**, "current" means: from the latest scan, not older than `staleAfterHours`, for this computer's platform, and from an area that scan was permitted to look at. Ranking inside a group: *present* beats *absent* beats everything else.

| Status | UI label (`observedLabel`) | Meaning — and what it does **not** mean |
|---|---|---|
| `present_current` | Son taramada bulundu | At least one **current** same-platform observation says "present" (a file/record with that name or declaration was found). Not installed, not identity-verified, not working, not enabled in the agent. |
| `absent_current` | Son taramada bulunamadı | No current "present", at least one current "absent". Only within what that scan was allowed to look at. |
| `unknown_current` | Son taramada belirlenemedi | Current observations exist but none says present or absent (unknown / unsupported). |
| `previously_present` / `_absent` / `_unknown` | Eski taramada … (güncel değil) | Same platform, but **no** current observation; the best older one is reported with the reason (older than the staleness limit, not produced by the latest scan, area not examined). "Eski bir sonuç hiçbir işlem için dayanak sayılmaz." |
| `platform_mismatch` | Başka bir işletim sistemi için gözlem var (bu tercih için geçerli değil) | Relevant observations exist only for another platform. |
| `not_scanned` | Henüz taranmadı | No relevant observation at all for this project/surface/platform. Says nothing about whether the tool exists. |

## Known limits of the contract (deferred, not invented)
* `RequestVsObservation` does not say **which** observation (project-specific, computer-wide, or a `local-cli` lookup) supported the status. The UI discloses that a computer-wide lookup may count for every project, but cannot tell which case applies to a row. Fixing that needs a Rust/contract change and is a backlog item.
* The backend gives no read timestamp, so the matrix cannot show "inventory read N minutes ago". "Old" is only claimed when a later refresh visibly failed; staleness of the *scan* is already in the status.
* The matrix deliberately omits raw evidence, locations, versions, ladder and fingerprint detail; those stay on the Bilgisayar Envanteri page under its existing explicit controls.
