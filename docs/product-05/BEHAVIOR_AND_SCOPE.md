# PRODUCT-05 — "İncelenmesi Gerekenler": a read-only review queue — behaviour and scope

Baseline: `development` at `b5ef0876cad6f91315dde49efcca30bf9cfa40f1` (PRODUCT-04). `main` stays at `68eda3f0407490fabccb517dcf822ab4818fc0bf`. Frontend only. **Not** production-ready; real installation remains NO-GO (Issues #14 and #25 open).

## What it is
A list, on the Projects page (above the matrix) and as one summary section on the Dashboard, of **saved decisions that deserve a second look**, so nobody has to open every project / tool / operating system / agent surface combination. It is derived — purely, in `src/review.ts` — from data the application already holds: each project's recorded wishes (`ProjectView.preferences`) and the stored discovery classification (`Inventory.requests`). It adds **no IPC command, no read the Projects page did not already make, no write, no scan.**
Its only action is **"Matriste incele"**: it hands the exact project + operating system + agent surface + tool to the Project Tool Workspace through the existing typed `MatrixHandoff` (the same contract Recommendations uses). Every decision stays in the matrix.

## The unit: one item per exact key
`project alias + tool (entry id) + operating system + agent surface`. Two records for the same key are one item, and the doubt is stated ("aynı hedef için birden fazla karar kaydı"). Evidence is joined by the same four fields (the PRODUCT-04 `requestMatches`; the queue calls the same `observationOfWish` as the matrix row, so the two can never disagree). Nothing is matched by tool id, display name or project name alone; the item names only its own source target.

## Reasons (a reason is a fact the saved data states; none is ranked)
| Reason | Fires when | Meaning shown — and what it does not mean |
|---|---|---|
| `material_change` | `PreferenceView.materialChange === true`, any decision | "Karardan sonra aracın yetenekleri veya uyumluluğu katalogda önemli ölçüde değişti." The catalog changed; nothing was observed on the computer. |
| `entry_removed` | `entryRemoved === true` | The tool left the catalog; the old decision stays. The matrix shows it as removed and offers only to look or reset. A removed entry is **never** compared with stored evidence. |
| `reminder_due` | `decision === "postponed"`, `remindAt` a valid RFC 3339 timestamp, `remindAt <= now` | "Hatırlatma tarihi geldi… karar hâlâ ‘Şimdi değil’." The date comes from the saved decision; `now` is a parameter (no timer, no polling). |
| `evidence` | **accepted**, non-removed wish whose exact-key backend status is one of `absent_current`, `unknown_current`, `previously_present`, `previously_absent`, `previously_unknown`, `platform_mismatch`, `not_scanned` | The backend's own classification (`observedLabel` + a gloss). `absent_current` / `unknown_current` are included because the backend's note for them is "you wanted it, the last scan did not find / could not determine it" — consistent with its semantics; each has its own wording. |

`present_current` is **not** a reason and is never called resolved, verified, installed or working: it is a stored discovery result that may come from a computer-wide or `local-cli` name lookup (the contract does not say which). Next to a real reason it is shown as plain information ("Kayıtlı gözlem: Son taramada bulundu"). A wish that is postponed, rejected or disabled is never compared with the stored evidence.
Reminder parsing is strict RFC 3339 (`parseRfc3339`): valid calendar day, hour ≤ 23, minute/second ≤ 59 (a leap second `:60` is not accepted), `Z` or `±hh:mm` offset applied; `Date.parse` is not used because it rolls "02-30" over to March and accepts date-only text. The boundary `now == remindAt` is **due**.

## Uncertain evidence — a separate section, not a verdict
An accepted, non-removed wish whose evidence cannot be used is **uncertain**, never "not scanned" / "not found":
* no matching backend row (`no_row`), two contradictory rows (`ambiguous`), a status string this version does not know (`unrecognised_status`);
* a postponed decision whose `remindAt` is unreadable (`reminder_unreadable`);
* a second record for one key (`duplicate_wish`).
An item with a real reason carries these as a "⚠ Kanıt belirsiz" line. An item with **only** uncertain findings goes to its own section "Kanıtı belirsiz kalan kararlar", is **not** counted as "needs review", and is never dropped. When a reason filter is on, that section is hidden *and the page says so*.
When the whole inventory is pending or failed (no earlier read) nothing per row is invented; the queue states once how many accepted wishes have unknown stored evidence.

## States — no false zero
| Decisions | Inventory | Headline |
|---|---|---|
| loading | — | "Kararlar okunuyor…" (no number) |
| failed (first read) | — | "Kararlar okunamadı; incelenecek öğelerin sayısı bilinmiyor. Bu, ‘hiç yok’ anlamına gelmez." + **Listeyi yeniden oku** |
| ready | loading / failed / old | "En az N öğe…", plus a warning; evidence reasons are absent (loading, failed) or possibly out of date (old); never the "nothing to review" text |
| old (data + failed refresh) | any | "En az N…" and "Kararlar yenilenemedi; … eski olabilir" |
| ready | ready | "N öğe incelenmeyi bekliyor." — and, for N = 0, "Bu ölçütlere göre incelenmesi gereken öğe yok. Bu, her şeyin doğru olduğu ya da herhangi bir aracın kurulu ve çalışır olduğu anlamına gelmez." |
Only "ready + ready" is *complete*. The Dashboard section uses the same `reviewHeadline`, so the two counts and caveats cannot differ.

## Viewing changes nothing
There is no acknowledge, dismiss, mark-seen or auto-clear, and no stored state of any kind. Opening or viewing an item never lowers a count. The count changes only when the saved decision, the catalog state, the clock or the stored evidence really changes: it can fall when a decision is changed in the matrix or the stored evidence changes, and it can *rise* when a reminder date is reached or the catalog changes. A test saves a real decision in the matrix and watches the count follow. The page says this, and says that a count is not a to-do list that is meant to reach zero. The clock is read once per answer, not on a timer: a reminder that falls due while the page stays open appears at the next read.

## Navigation ("Matriste incele")
* Button name: `Matriste incele (araç adı · araç kimliği; proje · işletim sistemi · ajan yüzeyi)` — the id tells same-named tools apart.
* The App sets the handoff `{alias, platform, surface, entryId, nonce}`; the matrix selects that target and marks, scrolls to and focuses that row. It selects no decision, saves nothing, resets nothing, evaluates nothing, scans nothing. The row shows the saved decision as it is, and (PRODUCT-04) the stored observation beside it.
* A removed tool opens in the existing removed / reset-only display.
* A target whose operating system or agent surface this version does not know is listed **without** a button and with the note "bu sürümde matriste açılamıyor; başka bir hedef seçilmedi". If a project vanishes between a read and a click, the existing matrix note applies and no other row is highlighted; after a refresh the project's items are gone from the queue (tested).
* Ordinary sidebar navigation clears the handoff (it cannot be applied twice).

## Display and privacy
Shown per item: tool name and id, project (example projects are tagged "örnek veri" and counted like any other, because the list does not know they are invented otherwise), operating system, agent surface, the wish (labelled as a recorded wish, not an installation), the reasons, and limited provenance (the finish time of the scan the status rests on). Never shown: the project's private note, any path, any raw observation location / evidence / version, any secret (tested with planted values). All text is React text interpolation; no HTML, no links.
Filters: by project and by reason (labels "Kuyrukta proje süz" / "Kuyrukta neden süz", distinct from the matrix's own selects). Filtering changes what is listed, not what exists ("x / y öğe gösteriliyor").

## What it does not claim
A wish, a stored finding, a catalog flag and a managed-installation record are four different things. The queue evaluates the first three as recorded and **does not read or judge the installation record or the compatibility**. Nothing here says a tool is installed, verified, working, authenticated or active in any agent.

## Boundaries (not done, on purpose)
No Rust, IPC, schema, file write or live scan was needed or added. No durable acknowledgement (it would need its own data model and approval). No severity ranking. No background refresh. The contract still cannot say whether a status rests on a project-specific, computer-wide or `local-cli` observation (backlog).
