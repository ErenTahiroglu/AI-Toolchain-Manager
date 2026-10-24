# PRODUCT-03 — context-safe recommendations, one place to decide

Baseline: `development` at `34ef2046a606f9ce12f14aca6c051d6ba4577212` (PRODUCT-02). `main` is frozen at `68eda3f0407490fabccb517dcf822ab4818fc0bf` and was not touched.
Frontend only. Nothing under `src-tauri/`, `.github/`, `package*.json`, `docs/phase-3d*` or the offline-preflight folder was changed, no IPC command, dependency, migration, capability or permission was added, and nothing was run that installs, downloads, launches or scans anything. The final commit SHA and the exact-SHA CI result are **not** written here (that would need a second commit); they are in the hand-off report.

## A. P02-RT-01 — an evaluation can no longer reappear under another context

### The defect (reproduced before the change)
`RecommendationsPage.evaluate()` awaited `api.evaluateTask(...)` and then unconditionally called `setResult(...)`. The cards were rendered from the *current* `alias / platform / surface` state, and each card's quick action wrote with those current values.
Reproduction on the unfixed code (`34ef204`, a deferred `evaluate_task`; test below, first assertion fails):
1. choose project A, type a task, press *Değerlendir* (request pending);
2. choose project B;
3. let A's answer arrive → A's card "Anthropic Frontend Design / Öneri" appears under B;
4. pressing its *Bu projede kullanmak istiyorum* sent
   `set_project_preference {"alias":"proje-b","entryId":"anthropic-frontend-design","agentSurface":"claude-code-terminal","platformTarget":"windows-x64","decision":"accepted","remindDays":null}` — a stored wish for **B** from an answer that was computed for **A**.
After the change the same test passes: no card, no handoff button, no result section, no loading indicator, and zero preference calls.

### The fix (`src/pages/Recommendations.tsx`, `src/evaluation.ts`)
* **Snapshot.** `evaluate()` freezes every input at the click into an `EvalSnapshot` (alias, OS, surface, task id, task text exactly as typed, evidence **file names**, phase, assumed plugin scope). The wire input (`toTaskInput`) is built from the snapshot only. The result is stored together with its snapshot.
* **Latest request wins.** A `generation` counter and an `inFlight` marker. `cancelPending()` bumps the generation, clears `inFlight` and ends `loading`; it is called by **every** input handler *before* the input changes (task text, file names, plugin scope, late phase, project, OS, surface, example task, *Yeni görev başlat*), when the selected project disappears, and (through the effect cleanup) on unmount. A settling request that no longer holds the current generation returns at once: it changes neither the result, nor the error, nor `loading`. Only the newest request can finish the indicator.
  *Design note:* a second guard (compare the form with the snapshot when the answer arrives) was written first and then removed, because mutation checks showed it only duplicated the generation guard on every reachable path, so neither could be verified alone. One mechanism remains. A new input must therefore call `edited()` (the comment says so; the per-input tests fail if one does not).
* **No retry, no silent rewrite.** A discarded answer is dropped; the page says so in a status line ("Girdileri değiştirdiğiniz için süren değerlendirmenin sonucu atıldı…"). The task text is sent as typed; file *names* are trimmed/split exactly as before and never opened.
* **Repeated clicks.** *Değerlendir* is disabled while a request runs, and `evaluate()` also returns if one is in flight (same-tick double click → one `evaluate_task`).
* **A discarded request gets a new task id.** `evaluate_task` records "this tool was shown for (project, task id)" and then refuses to show it again for that task. A request that is discarded after the backend answered may already have recorded suggestions nobody saw; reusing its task id would silently suppress them in the next attempt. `cancelPending()` therefore issues a new task id when the discarded request used the current one (a test asserts the next request has a different id). Completed results keep their task id, so the intended "late suggestion" flow (start → tick *late* → evaluate again, same task) is unchanged.
* **Answer validation.** The answer must carry the task id and phase that were asked about and every recommendation an entry id; otherwise `EVALUATION_ANSWER_MISMATCH` is shown ("… Sonuç gösterilmedi.") and nothing is rendered. (The Rust command echoes both; the test mock now does too.)
* **A finished result stays bound to its inputs.** Changing project, OS or surface starts a new task and clears the result (unchanged). Changing only text, file names, phase or plugin scope keeps the result visible but marked: "⚠ Bu sonuç formdaki önceki girdilerle üretildi; şunları değiştirdiniz: …". The header always names the evaluated target ("Değerlendirilen hedef: Proje A · Windows x64 · …").
* **A project that disappears.** If the backend answers `PROJECT_NOT_FOUND`, the page re-reads `list_projects`; if the selected alias is gone it cancels anything pending, clears the result and the error, picks the first remaining project (or none) **and says so** ("Seçili proje … artık listede yok; sonuçlar temizlendi…"). If the project still exists the error is shown as before.

### What was *not* changed
`evaluate_task` semantics and the Rust side. The command still writes its documented "already shown for this task" suggestion-log row for every non-discarded **and** every discarded-in-flight request (the backend cannot be told to forget). This is **not** a zero-write evaluation, and the Recommendations page now says so in its intro ("her değerlendirme, hangi önerinin bu görevde gösterildiğini yalnızca bu bilgisayardaki uygulama kaydına yazar; tercih kaydedilmez"). Requested versus actual data: the *requested* values are what the user typed/selected and are what is sent; the *actual* answer is only the backend's deterministic rule result for that snapshot; nothing was observed on the computer, and the "Frontend Design is already installed" selector is an assumption the user supplies.

## B. One place to decide

* **The second writing path is gone.** `RecommendationCard` no longer has `decide()`, its three buttons, its `done`/`error` state or an `api.setProjectPreference` call. `Recommendations.tsx` references neither `setProjectPreference` nor `resetProjectPreference` (source-scan test), and a behavioural test presses every control on the page after a result and asserts zero preference/install/network calls. The only decision surface is the Project Tool Workspace row (`setProjectPreference` / `resetProjectPreference` there, backend authoritative, `acceptBlockReason` still advisory).
* **Each card carries "Bu öneriyi matriste incele"** (accessible name adds *card tool; project · OS · surface* of the **evaluation's snapshot**, not of the form) and the existing rationale, impact, signals, notes, skill candidates and the "Yalnızca bilgi" badge are unchanged. A line states: informational only, nothing is saved/installed/enabled here, the decision is made in the matrix; for `canEnable == false` it says why "kullanmak istiyorum" is not selectable in the matrix.
* **Handoff contract.** `MatrixHandoff = { alias, platform, surface, entryId | null, nonce }` (typed callback through `App`; no router, no storage). `App` clears it on any ordinary sidebar navigation. The target-only button ("Bu hedefi Proje Araç Matrisi’nde aç") keeps working with `entryId = null`.
* **Matrix behaviour** (`resolveEntryFocus`, pure; `ProjectToolWorkspace`): the request waits for projects and catalog, lapses if the user picks another target first, never selects a different tool, and never writes or evaluates.

| Situation | Result |
|---|---|
| tool exists in the target | row gets a text tag "Öneriden gelen araç", a heavier border, is scrolled into view and **focused**; status line "Öneriden açıldı: … henüz karar vermediniz ve hiçbir şey kaydedilmedi…"; nothing is pre-selected |
| a filter hides the row | filters are cleared and the user is told |
| unknown compatibility / blocked / catalog-disabled | row shown; note that "kullanmak istiyorum" is not selectable (the option is disabled; compatibility still reads "Bilinmiyor") |
| removed from catalog **with** a decision | row shown, marked removed, no new decision possible |
| removed from catalog **without** a decision, or id not in catalog | no row is highlighted, a note names the tool, no other tool is chosen |
| project deleted / unknown OS or surface | existing fallback note **plus** "target not found" note; nothing highlighted |
| catalog cannot be read | note "Önerilen araç satırı vurgulanamadı."; nothing selected |
| user changes target afterwards | mark and note disappear and do not return when the user comes back |
| sidebar navigation, repeat visit | no mark, no note, no focus; a confirmation from an earlier target is never shown at another |

* **Deliverable C — `useLoad.setData`.** `setData` now bumps the load generation and ends `loading`, so a read started earlier can no longer overwrite a value set afterwards (success or failure); a reload started *after* `setData` wins normally. API unchanged (`data, error, loading, reload, setData`, functional updates, stable identity); harmless after unmount. No cache or query layer.
* **Small a11y fix found by a test:** the accessible name of buttons with a visually hidden suffix lacked the space before the suffix ("…aç(Benim sitem)"); fixed for the new button and for the Projects card button.

## C. Tests (all through the in-memory IPC mock + jsdom)

| File | Tests | What it adds |
|---|---|---|
| `src/Recommendations.test.tsx` (new) | 22 | reproduction; A/B overlap in both settlement orders; stale rejection after target change; stale error after newer success; stale success after newer failure; answer-for-another-task refusal; **per-input** invalidation (text, file names, plugin scope, late phase, surface, OS, project, task reset, example task); what the 2nd request really sends; double click; indicator ownership; leaving the page (success and failure); lost project; result bound to its snapshot / outdated note; no second write path (source scan + press-everything + command allow-list). An `afterEach` asserts zero `set/reset_project_preference` in every test. |
| `src/EntryHandoff.test.tsx` (new) | 21 | the table above, through the real sidebar/Recommendations flow and, for states the UI cannot produce, `ProjectsPage` fed the typed handoff; keyboard Enter; repeated handoff; no auto-select / no save / no evaluation; the actual save only on the matrix button with the full key. |
| `src/evaluation.test.ts` (new) | 7 | snapshot → wire input, file-name parsing, answer validation, change description. |
| `src/useLoad.test.tsx` | +5 | deliverable C. |
| `src/workspace.test.ts` | +9 | `exact`, `resolveEntryFocus`. |
| `src/App.test.tsx`, `src/Handoff.test.tsx` | 3 migrated | The three tests that pinned the old second write path were **re-expressed**, not deleted: the previously permitted decisions (accepted / postponed / rejected, same key) are now made in the matrix after the handoff; informational/Strix cards carry no decision control and the matrix keeps "kullanmak istiyorum" unavailable while "reddet" still saves; the old Handoff quick-decision test became "a card opens the matrix on that tool and records nothing". No other test was edited. |

Totals: `npx vitest run` → 11 files, **280 passed, 0 failed, 0 skipped** (baseline 209); no `.skip/.only/todo` in `src/`.

### Mutation checks (one change at a time, hash-verified restore, full suite)
Caught: generation guard removed (16 failing); `cancelPending` not bumping (11); `cancelPending` leaving *loading* (17); repeated-click guard removed (12); discarded task id reused (4); handoff drops tool id (12); hiding filter not cleared (2); replaced target still highlights (3); sidebar navigation keeps handoff (2); `setData` not invalidating (3); wrong-task answer accepted (2); mark not cleared on target change (1); lost-project handling removed (1); a writing control added to a card (3); handoff auto-saving `accepted` (7).
**Not caught — reported, not hidden:**
* *Cards pointing at the current form instead of their snapshot*: equivalent mutant today, because changing project/OS/surface clears the result in the same event, so form and snapshot cannot differ while a card is on screen. The binding is structural defence; the accessible-name test pins the snapshot values only for states that exist.
* *Mark tied to the target at render time*: redundant with the clearing effect (only a one-render difference).
* *Unmount cleanup removed*: not observable here — React 19 ignores state updates on unmounted components and `useLoad.reload` is already guarded. The cleanup is kept because it was required and costs nothing; the tests assert no console error and no side effect after leaving the page.

### Commands run locally
`npm run typecheck` exit 0 · `npx vitest run` exit 0 (280/280) · `npm run build` exit 0 · `python -X utf8 -m unittest discover -s tests` exit 0 (286 ran, OK, 58 skipped — unchanged from PRODUCT-02; reasons not itemised here).

### Not run / not claimed
`cargo fmt/clippy/test`, `tauri build`, `--self-check` (Rust untouched; the exact-SHA CI runs them), any real window, WebView2, port 9222, jq, the offline-preflight harness. jsdom has no layout, focus ring rendering, scrolling, dark-theme contrast, narrow-window collapse or screen reader: the focus/scroll/highlight and the live-region wording are verified only as DOM state. No claim about macOS GUI, a real install, network egress or WebView2 profile isolation.

## D. Known gaps
* The evaluation suggestion log is still written for discarded requests (see A); only the UI is context-safe.
* An "outdated" result stays actionable as a *navigation* to its own snapshot's target; it cannot write.
* `acceptBlockReason` remains an advisory mirror of the backend rules.
* Turkish copy has had no editorial review; README "Phase 3D-0" banner still stale (unchanged, not in scope).
* Issue #14/#25 remain open; no issue is closed or edited. Candidates for independent decision: none claimed.
