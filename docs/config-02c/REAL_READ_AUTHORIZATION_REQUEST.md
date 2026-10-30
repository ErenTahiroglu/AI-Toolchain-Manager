# CONFIG-02C — request for the owner's decision on ONE real read operation

Issue #40. **This is a request, not a grant. `READ_NOT_GRANTED`.** Reading this document, merging it, or a green CI run authorises nothing. Every box in §8 is unchecked; the default answer to every question is **NO**. Approval, if given, must be recorded by the owner in Issue #40 in their own words, naming the decisions below, and applies **only** to what it names (approval in one context does not extend to another).

Technical basis: `READ_ONLY_CONTRACT.md` (§-numbers refer to it) and `THREAT_AND_PRIVACY_TEST_PLAN.md`. Not in scope and not requested: any write, Apply control, installation, ECE verification (#34), GitHub publication (#35), jq pilot, GUI/WebView2 pilot (#25), or resolution of #14.

## 1. What is being asked, in one paragraph

Whether to allow a later implementation phase to add **one new read-only command** that, **after an explicit click and an explicit confirmation text**, opens **one folder the user typed** and looks **at one file inside it — `.mcp.json`, at most 1 MiB, once** — and returns to the screen only a small **closed** result: whether the file exists, its size, its SHA-256, and — for one catalog declaration (Exa) — whether a server of that name or endpoint is already present, plus the number of servers. It returns **no text from the file**, stores **nothing**, writes **nothing**, and does **not** touch the pasted-text preview. A **NO** leaves the application exactly as it is today.

## 2. What already exists (so the decision is not made on a wrong premise)

The application **already** has a path-accepting command, `run_discovery_scan`, which — when the user ticks "Bir proje klasörüne bak" and types a folder — reads `.mcp.json` (and two `.claude/settings*.json` files) and keeps only a count of matches to one catalog entry. That was shipped earlier and is **not** what is being requested here, and this request does **not** rely on it as precedent for the new command. What is new: a dedicated command, a digest, a closed status vocabulary, hard-link/case-alias/identity checks the old reader does not make, no persistence, and the reviewed-declaration summary. See contract §0 (F1–F5), and open item O1 about the two readers' differing verdicts.

## 3. The decisions (each is independent; answer YES or NO to each)

### D1 — Add one typed read command?
Name (tentative): `read_project_mcp_config`; one new permission `allow-read-project-mcp-config` in `capabilities/default.json`. **No other capability, no plugin permission, no new dependency, no migration.** The command is registered in the same allow-list as the other 36.

### D2 — How the project root is chosen

| Option | What it is | New authority | Recommendation |
|---|---|---|---|
| **A** | The user **types** the absolute folder path in a text box, per read; used for that call only and cleared when the call returns (same convention as the Inventory scan). | none beyond D1 | **Recommended for the first step** |
| B | A **Rust-side native folder picker** (new `tauri-plugin-dialog`/`rfd` dependency), the path never reaching the WebView | new dependency + plugin initialisation; cannot be exercised by headless CI; needs a GUI pilot (not authorised, #25) | later, separately |
| C | Reuse the Inventory scan (extend it) | none | **Not recommended**: persists the result, bundles three files, wrong place, weaker Windows reader |

Neither A nor B stores a path or a root identity. Storing an identity (registry column, migration) is **not requested** and is a precondition of any future *write* (`FUTURE_WRITE_GATE.md` §2), not of this read.

### D3 — Which file and how much
Exactly the constant relative name **`.mcp.json`**, directly in the typed folder, **never** a sub-folder or another name; **1,048,576 bytes** maximum (larger ⇒ not read); local filesystems only; no link, junction, hard link or case-variant name accepted. Surface: *Claude Code (terminal)*, scope: project. Antigravity, Codex, `~/.claude.json`, settings files, `CLAUDE.md` are **not** included.

### D4 — What crosses IPC (complete list; nothing else may)

| Field | Content |
|---|---|
| `status` (+ `reason`) | one value of a closed list of 11 (`root_not_found`, `absent`, `present_summarized`, `present_metadata_only`, `malformed`, `denied`, `unsafe_path`, `unsupported_fs`, `too_large`, `changed_during_read`, `io_error`) |
| `sizeBytes`, `sha256` | only when the whole file was read |
| `serverCount` | only for `present_summarized` |
| `reviewedDeclarations` | for the reviewed Exa declaration only: `nameState`, `endpointState` from closed lists |
| constants | schema id, file name, scope, surface, byte cap, the six "not claimed" codes |
| `alias` | the alias the UI already sent |

**D4a — YES/NO: accept that no server name other than the reviewed one is returned.** (Recommended YES.)
**D4b — YES/NO (later, separate): allow user-chosen server *names* to be shown** under a strict grammar `^[A-Za-z0-9][A-Za-z0-9_-]{0,31}$`, at most 16, ephemeral display only, never logged or stored. Risk: a secret placed in a name would be shown on screen. **Not part of this request**; listed so the question is asked once and answered NO unless the owner says otherwise.

### D5 — How often, how long
One explicit click = one read. **No** automatic read, scan, timer, file watcher, retry loop, background task, or read on startup/navigation. The result lives in memory while the panel is mounted and is cleared by *Temizle*, navigation, a change of project/OS/surface/entry, a new read, or **5 minutes**. Nothing is written to the database, diagnostics, audit chain, settings, clipboard or disk. (Optional D5a — YES/NO: write a diagnostics line containing only `alias` and `status`. Recommended **NO**.)

### D6 — Which filesystem primitive
A new **read-only** child module of `managed_fs` using its handle-relative, no-follow root walk (the read half of the 02B `inspect`, re-implemented and re-reviewed; none of its write, staging, backup or rollback code). This is the layer's first production use. It does not touch the deletion paths of Issue #14, does not resolve #14 and does not remove the `#![allow(dead_code)]` reason for any other function. Alternative: extend `discovery::fsx` (already in production; weaker on Windows — path-based, no link-count, no identity). **Recommended: managed_fs read half.** Open item O2 asks the reviewer to confirm.

### D7 — Evidence required before the implementation may be accepted
All of: the tests of `THREAT_AND_PRIVACY_TEST_PLAN.md` §3 implemented, with `SKIPPED` visible per ID in CI output (§0 of the plan); exact-SHA CI green on **Windows x64 and macOS arm64**; a report that states PASS/SKIPPED/BLOCKED/NOT RUN per ID per OS; an independent Red Team pass using §5 of the plan; no new write-capable call in the module (plan G2). A green CI run is supporting evidence, not acceptance.

### D8 — Explicitly forbidden (in this request and in the implementation)
* Writing, creating, renaming, deleting, touching timestamps of, or adding a temporary/backup file to the user's project, or anything else.
* Any file other than `.mcp.json`; any directory listing shown or stored; any recursion; scanning the user's home, Downloads, Documents or any default root.
* Returning, logging, storing, caching, copying or exporting file text, an excerpt, a line, a parse error, an unknown field, a URL, header, env key/value, command, argument, or any server name beyond D4.
* Any network call (including to Exa, Claude, GitHub), process spawn, MCP connection, package install, agent launch, clipboard operation, background watcher, telemetry.
* Any claim that the declaration is loaded, trusted, installed, enabled, reachable, safe or verified; deriving "not configured" from a missing, denied, unsafe or unreachable file.
* Using the result to change a preference, a catalog acceptance, an install state or the review queue; using the digest as the expected state of any write.
* Enabling any Apply control; promoting the 02B test-only writer; touching `REAL_CONFIG_WRITE_NO_GO`.

### D9 — Saying NO
* **NO to D1** (or to the whole request): nothing is built. The pasted-text preview stays as it is. The existing scan stays as it is.
* **YES to D1 but the user declines at the gate**: the button *Hayır, metni yapıştırarak devam edeceğim* closes the section; no read occurs; no state is kept; the preview is unaffected.
* A user who never opens the section is never asked anything.

## 4. User-visible Turkish wording (proposed; subject to review — it must never promise more than the implementation does)

**Section title (inside "Yapılandırma önizlemesi", collapsed by default):**
`Dosyadan oku (isteğe bağlı) — yalnızca .mcp.json, bir kez, salt okunur`

**Folder field label:** `Proje klasörünün tam yolu (yalnızca bu okuma için kullanılır, kaydedilmez, okuma biter bitmez silinir)`
Example placeholder: `C:\Projeler\sitem veya /Users/ad/sitem`

**Confirmation text (shown before the button; read verbatim from a constant):**

> **Bu uygulama, yazdığınız klasördeki yalnızca `.mcp.json` dosyasına bir kez bakacak.**
> - **Okunacak dosya:** `.mcp.json` — klasörün hemen altında. Başka dosya, alt klasör veya üst klasör okunmaz. Kapsam: proje (Claude Code, terminal).
> - **Sınır:** en çok 1 MiB (1.048.576 bayt). Daha büyük dosya okunmaz.
> - **Yerel:** dosya yalnızca bu bilgisayarda işlenir. İnternete bağlanılmaz, hiçbir program çalıştırılmaz, hiçbir dosyaya yazılmaz, hiçbir şey kurulmaz.
> - **Ekrana yalnızca şunlar gelir:** dosyanın var olup olmadığı, boyutu, SHA-256 özeti, onaylı Exa bildirimiyle ilgili durum ve toplam sunucu sayısı. **Dosyanın metni, anahtarlar, parolalar, URL’ler, ortam değişkenleri, başlıklar ve diğer sunucuların adları ekrana, günlüğe veya veritabanına gelmez.**
> - **Saklanmaz:** sonuç yalnızca bu sayfada, bellekte durur; “Temizle”ye bastığınızda, sayfadan ayrıldığınızda veya 5 dakika sonra silinir.
> - **Ne anlama gelmez:** bu okuma, bir aracın yüklendiğini, güvenilir olduğunu, kurulu veya etkin olduğunu, erişilebildiğini ya da çalıştığını göstermez. Dosya, okuma anındaki bir anlık görüntüdür.
> - Dosya hâlâ kendi düzenleyicinizde görünür; bu uygulama onu değiştirmez.

**Buttons (no default focus on the first):**
`Bu klasördeki .mcp.json dosyasını bir kez oku` · `Hayır, metni yapıştırarak devam edeceğim`

**Result messages (one per status; the first line of each result):**

| Status | Text |
|---|---|
| (UI) `not_requested` | `Henüz okunmadı. Hiçbir dosyaya bakılmadı.` |
| `root_not_found` | `Bu klasör bulunamadı; dosyaya bakılmadı. Bu, dosyanın olmadığı anlamına gelmez.` |
| `absent` | `Bu klasörde okuma anında .mcp.json adlı bir dosya yoktu. Bu, MCP’nin başka bir yerde (kullanıcı veya yerel kapsam) yapılandırılmadığını göstermez.` |
| `present_summarized` | `.mcp.json bulundu ve okundu: {n} sunucu tanımı. Hiçbir sunucu çalıştırılmadı; dosyanın içeriği saklanmadı.` |
| `present_metadata_only` | `.mcp.json bulundu ancak içeriği güvenle özetlenemedi; yalnızca boyut ve SHA-256 özeti gösteriliyor. Bu, Claude Code’un dosyayı geçersiz bulacağı anlamına gelmez.` + reason line |
| `malformed` | `.mcp.json geçerli bir JSON metni olarak okunamadı; yalnızca boyut ve SHA-256 özeti gösteriliyor. Claude Code’un dosyayı nasıl yorumlayacağı hakkında bir şey söylenmez.` |
| `denied` | `Bu dosyaya veya klasöre erişim izni yok ya da dosya başka bir program tarafından kilitli. Dosyanın var olup olmadığı bilinmiyor; “yapılandırılmamış” anlamına gelmez.` |
| `unsafe_path` | `Güvenlik için okunmadı: yol ya da dosya bir sembolik bağlantı/junction, sabit bağlantı, normal olmayan bir nesne ya da yalnızca harf büyüklüğüyle ayrılan ikinci bir ad içeriyor.` + reason line |
| `unsupported_fs` | `Güvenlik için okunmadı: klasör, yerel olduğu doğrulanan bir dosya sisteminde değil (ağ paylaşımı, uzak sürücü veya bilinmeyen dosya sistemi).` |
| `too_large` | `Dosya 1 MiB sınırından büyük; okunmadı.` |
| `changed_during_read` | `Dosya okunurken değişti; hiçbir sonuç gösterilmedi. Başka bir program (düzenleyici, git, ajan) dosyaya yazıyor olabilir. İsterseniz yeniden deneyin.` |
| `io_error` | `Dosya okunamadı (beklenmeyen hata). Hiçbir sonuç gösterilmedi.` |

**Persistent footer under every result:** `Okuma anındaki anlık görüntüdür; dosya şimdi değişmiş olabilir. Bu sonuç yüklendiğini, güvenildiğini, kurulduğunu, etkin olduğunu, erişilebildiğini veya doğrulandığını göstermez. [Temizle]`

**Paste bridge (only if the user also pasted text):**
equal → `Yapıştırdığınız metin bu dosyayla bayt bayt aynı.` · different → `Özetler farklı; yapıştırdığınız metnin bu dosyayla aynı olduğu kanıtlanamadı (satır sonu veya kodlama farkı olabilir).`

## 5. What the owner is *not* being asked to trust

* That the typed folder is "the project's": the pairing is the user's assertion (contract §4, test A6).
* That the digest identifies the file's origin: it identifies the bytes read.
* That the reader's strictness matches Claude Code's.
* That macOS behaves as designed: it is design-only until a macOS CI run executes the tests (plan §4).
* That any write is now closer: `REAL_CONFIG_WRITE_NO_GO` is unchanged, and the Windows silent lost-update, absent lock, absent recovery journal and absent permission/ACL preservation of the 02B prototype remain open blockers for any write.

## 6. What happens after each answer

| Answer | Next step |
|---|---|
| NO to D1 | #40 stays a design; nothing is built |
| YES to D1–D3, D4a, D5–D8 with D2 = A | A separate implementation phase starts; its report must follow D7; the phase ends at "implemented, independently reviewed", not at "granted" |
| YES with D2 = B | A separate approval for the picker dependency and a GUI pilot is required first |
| Anything unclear | NO |

## 7. Residual blockers regardless of the answer

#14 and #25 are open. #39's Windows lost-update is a hard NO-GO for writing. The independent review's open items O1–O5 (contract §11) are unresolved. The local Windows Rust test binary is blocked by Smart App Control, so Rust behaviour here is evidenced by CI only.

## 8. Owner response (to be completed by the owner; nothing is pre-filled)

```
Date: ____________            Reviewed SHA: ____________________________________________

D1  [ ] YES   [ ] NO          D2  [ ] A   [ ] B   [ ] NO read at all
D3  [ ] YES   [ ] NO          D4a [ ] YES [ ] NO    D4b [ ] NO (default)  [ ] YES (separate phase)
D5  [ ] YES   [ ] NO          D5a [ ] NO (default)  [ ] YES
D6  [ ] managed_fs read half  [ ] extend fsx        D7  [ ] YES   [ ] NO
D8  [ ] confirmed in full     D9  [ ] confirmed

Conditions / changes to the wording above: __________________________________________

Owner decision recorded in Issue #40 as: [ ] comment link ____________________
Status: READ_NOT_GRANTED   (changes only by a new commit that edits this line and the guard test)
```
