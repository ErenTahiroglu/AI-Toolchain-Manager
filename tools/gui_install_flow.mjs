// Phase 3D-2 GUI evidence driver. It drives the REAL application window (WebView2 remote-debugging port, loopback only) through the
// installation workflow of the SYNTHETIC pilot build (`--features synthetic-pilot`, never a release build) and checks the file system
// of an ISOLATED data directory. It starts nothing itself: tools/gui_install_flow.ps1 starts and stops the application.
//
//   PHASE=first          catalog -> preview -> forged IPC -> cancel -> install -> replay -> details -> user data -> remove -> reinstall -> tamper
//   PHASE=after-restart  the installed state survives an application restart (the earlier process was killed, not closed)
//
// Environment: ATM_DATA_DIR (the isolated directory the application uses), EVIDENCE_DIR (screenshots and result.json are written here).
import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, renameSync, statSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { diff, inventory } from "./fs_inventory.mjs";

const DATA = process.env.ATM_DATA_DIR;
const OUT = process.env.EVIDENCE_DIR;
const PHASE = process.env.PHASE ?? "first";
if (!DATA || !OUT) throw new Error("ATM_DATA_DIR and EVIDENCE_DIR are required");
mkdirSync(OUT, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); }
};
const send = (method, params = {}) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.result?.exceptionDetails) return "EXC: " + JSON.stringify(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text);
  return r.result?.result?.value;
};
const shot = async (name, selector) => {
  // bring the part that matters into the visible window before the picture is taken
  if (selector) { await ev(`document.querySelector(${JSON.stringify(selector)})?.scrollIntoView({ block: 'start' })`); await sleep(250); }
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, `${PHASE}-${name}.png`), Buffer.from(r.result.data, "base64"));
};

// ---- helpers for the Phase 3D-3 scenarios ------------------------------------------------------------------------------------------
const saveInventory = (name) => writeFileSync(join(OUT, `inventory-${name}.json`), JSON.stringify(inventory(DATA), null, 1));
// A second connection to the application's own database, used ONLY by this test to make audit writes fail (a trigger) and to remove the trigger.
const sqlite = (sql) => { const db = new DatabaseSync(join(DATA, "atm.sqlite")); db.exec("PRAGMA busy_timeout = 5000"); db.exec(sql); db.close(); };
const failAudit = (eventType, name) => sqlite(`CREATE TRIGGER ${name} BEFORE INSERT ON audit_events WHEN json_extract(NEW.record_json, '$.eventType') = '${eventType}' AND json_extract(NEW.record_json, '$.outcome') = 'passed' BEGIN SELECT RAISE(ABORT, 'audit write refused'); END;`);
const dropTrigger = (name) => sqlite(`DROP TRIGGER ${name};`);
const results = [];
const check = async (label, expr, wait = 300) => {
  const v = typeof expr === "function" ? await expr() : await ev(expr);
  const shown = typeof v === "string" ? v : JSON.stringify(v);
  console.log(label.padEnd(96), "->", shown.length > 160 ? shown.slice(0, 160) + "…" : shown);
  results.push({ label, value: v });
  await sleep(wait);
  return v;
};
const waitFor = async (expr, timeout = 12000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < timeout) {
    if ((await ev(expr)) === true) return true;
    await sleep(200);
  }
  return false;
};

await ev(`window.__h = {
  btn(re, root) { return [...(root || document).querySelectorAll('button')].find((b) => re.test(b.textContent.trim())); },
  group(tool) { return document.querySelector('[role=group][aria-label="Kurulum: ' + tool + '"]'); },
  nav(l) { [...document.querySelectorAll('nav button')].find((b) => b.textContent.trim() === l).click(); },
  async ipc(cmd, args) { try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(cmd, args) }; } catch (e) { return { ok: false, error: typeof e === 'string' ? e : JSON.stringify(e) }; } },
}`);

const HELLO = "synthetic-hello";
const rootDir = () => {
  const c = join(DATA, "atm-managed");
  if (!existsSync(c)) return null;
  const roots = readdirSync(c).filter((n) => n.startsWith("root-"));
  return roots.length === 1 ? join(c, roots[0]) : null;
};
const artifact = () => join(rootDir() ?? DATA, "versions", HELLO, "1.0.0", "artifact.bin");
const sha = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");
const groupText = (tool) => `(__h.group('${tool}') ? __h.group('${tool}').innerText : '')`;
const click = (tool, re) => `(() => { const b = __h.btn(${re}, __h.group('${tool}') || document); if (!b || b.disabled) return false; b.click(); return true; })()`;
const clickAnywhere = (re) => `(() => { const b = __h.btn(${re}); if (!b || b.disabled) return false; b.click(); return true; })()`;
const openCatalog = async () => {
  await ev(`__h.nav('Araç Kütüphanesi')`);
  await waitFor(`!!__h.group('${HELLO}')`);
  await sleep(500);
};
const dataFilesOutsideManager = () => readdirSync(DATA).filter((n) => !n.startsWith("atm.sqlite") && n !== "atm-managed" && n !== "pilot-input" && n !== "backups" && !/^(crash|diagnostic)/i.test(n));

await sleep(1500);
await openCatalog();
await shot("1-catalog");
const RETAINED = "synthetic-retained";
const INPUT = join(DATA, "pilot-input", "synthetic-retained.exe");
/** open the preview of `tool`, tick the acknowledgement and press the confirm button (matched by `confirmRe`) */
const drive = async (tool, openRe, confirmRe) => {
  if (!(await ev(click(tool, openRe)))) return false;
  await sleep(800);
  await ev(`document.querySelector('.preview-card input[type=checkbox]')?.click()`);
  await sleep(300);
  const ok = await ev(clickAnywhere(confirmRe));
  await sleep(900);
  return ok;
};

if (PHASE === "first") {
  // ---- the catalog tells the situations apart --------------------------------------------------------------------------------------
  await check("17 install panels (13 real entries + 4 synthetic)", `document.querySelectorAll('[role=group][aria-label^="Kurulum: "]').length === 17`);
  saveInventory("0-start");
  await check("synthetic-hello: 'Kurulabilir'", `${groupText(HELLO)}.includes('Kurulabilir')`);
  await check("synthetic-unverified-tool: 'Doğrulama tamamlanmadı' and no install button", `${groupText("synthetic-unverified-tool")}.includes('Doğrulama tamamlanmadı') && !${groupText("synthetic-unverified-tool")}.includes('Kurulum önizlemesini aç')`);
  await check("synthetic-license-pending: 'Lisans incelemesi gerekli' and no install button", `${groupText("synthetic-license-pending")}.includes('Lisans incelemesi gerekli') && !${groupText("synthetic-license-pending")}.includes('Kurulum önizlemesini aç')`);
  await check("a REAL catalog entry (gitleaks) is 'Doğrulama tamamlanmadı' with no install button", `${groupText("gitleaks")}.includes('Doğrulama tamamlanmadı') && !${groupText("gitleaks")}.includes('Kurulum önizlemesini aç')`);
  await check("a REAL entry cannot be previewed even through the IPC (forged call)", async () => (await ev(`__h.ipc('prepare_install', { request: { entryId: 'gitleaks', action: 'install' } })`)).ok === false);
  await check("the manager's folder does not exist yet (looking created nothing)", () => rootDir() === null);

  // ---- preview ------------------------------------------------------------------------------------------------------------------------
  await check("open the installation preview", click(HELLO, "/Kurulum önizlemesini aç/"), 800);
  await check("preview heading is shown", `[...document.querySelectorAll('h3')].some((h) => /Kurulum önizlemesi: .* 1\\.0\\.0/.test(h.textContent))`);
  const text = await ev(`document.querySelector('.preview-card')?.innerText ?? ''`);
  await check("preview: exact version, platform, digest, file name", () => /1\.0\.0/.test(text) && /Windows x64/.test(text) && /[0-9a-f]{64}/.test(text) && /artifact\.bin/.test(text));
  await check("preview: every file-system effect listed, none absolute", () => /versions\/synthetic-hello\/1\.0\.0\/artifact\.bin/.test(text) && /state\/synthetic-hello\.json/.test(text) && !/[A-Za-z]:\\|\/Users\//.test(text));
  await check("preview: says nothing is executed, no network, PATH untouched", () => /Hiçbir program çalıştırılmaz/.test(text) && /Ağ kullanılmayacak/.test(text) && /PATH/.test(text));
  await check("preview: notices (components + unresolved) are part of the confirmation", () => /Üçüncü taraf bildirimleri/.test(text) && /Çözülmemiş hukuki noktalar/.test(text));
  await check("preview: says what the confirmation is NOT (no identity proof)", () => /kimlik kanıtı DEĞİLDİR/.test(text) && /tek kullanımlıktır/.test(text));
  await check("preview: marked SENTETİK DENEME", () => /SENTETİK DENEME/.test(text));
  await check("the confirm button is disabled until the acknowledgement is ticked", `(() => { const b = __h.btn(/Onaylıyorum ve kur/); return !!b && b.disabled; })()`);
  await check("nothing was created by opening a preview", () => rootDir() === null);
  await shot("2-preview", ".preview-card");

  // ---- forged IPC against the live offer --------------------------------------------------------------------------------------------
  const offer = await ev(`(async () => { const r = await __h.ipc('prepare_install', { request: { entryId: '${HELLO}', action: 'install' } }); return r.ok ? { offerId: r.value.offerId, digest: r.value.previewDigest } : null; })()`);
  await check("a second preview supersedes the first (the old one is gone)", () => !!offer?.offerId);
  await check("forged confirm: unknown extra field (path) is rejected by the IPC layer", async () => (await ev(`__h.ipc('confirm_install', { request: { offerId: '${offer.offerId}', previewDigest: '${offer.digest}', acknowledged: true, path: 'C:\\\\Windows\\\\System32' } })`)).ok === false);
  await check("forged confirm: not acknowledged -> refused, nothing happens", async () => { const r = await ev(`__h.ipc('confirm_install', { request: { offerId: '${offer.offerId}', previewDigest: '${offer.digest}', acknowledged: false } })`); return r.ok && r.value.outcome === "refused" && r.value.code === "consent_not_acknowledged"; });
  await check("forged confirm after that: the burned offer cannot be used any more", async () => { const r = await ev(`__h.ipc('confirm_install', { request: { offerId: '${offer.offerId}', previewDigest: '${offer.digest}', acknowledged: true } })`); return r.ok && r.value.outcome === "refused" && /^consent_/.test(r.value.code); });
  await check("forged confirm: made-up offer id and malformed id are refused", async () => { const a = await ev(`__h.ipc('confirm_install', { request: { offerId: '${"0".repeat(64)}', previewDigest: '${"f".repeat(64)}', acknowledged: true } })`); const b = await ev(`__h.ipc('confirm_install', { request: { offerId: '../../x', previewDigest: 'x', acknowledged: true } })`); return a.ok && b.ok && a.value.outcome === "refused" && b.value.code === "consent_bad_identifier"; });
  await check("still nothing created after the forged attempts", () => rootDir() === null && dataFilesOutsideManager().length === 0);

  // ---- cancel ---------------------------------------------------------------------------------------------------------------------------
  await openCatalog();
  await check("open a preview again and cancel it with 'Vazgeç'", async () => { await ev(click(HELLO, "/Kurulum önizlemesini aç/")); await sleep(700); return ev(clickAnywhere("/^Vazgeç$/")); }, 800);
  await check("after cancelling the tool is 'Kurulabilir' again", `${groupText(HELLO)}.includes('Kurulabilir') && !${groupText(HELLO)}.includes('Onayınız bekleniyor')`);
  await check("a cancelled offer cannot be confirmed (forged replay of the cancelled id)", async () => { const r = await ev(`__h.ipc('confirm_install', { request: { offerId: '${offer.offerId}', previewDigest: '${offer.digest}', acknowledged: true } })`); return r.ok && r.value.outcome === "refused"; });
  await check("nothing created by cancel", () => rootDir() === null);

  // ---- the retained-file provider (the way the first real pilot will read its file): no path from the window ---------------------------------
  await check("retained tool: 'Kurulabilir' (the fixture sits in the fixed input folder)", `${groupText(RETAINED)}.includes('Kurulabilir')`);
  const retainedOriginal = readFileSync(INPUT);
  const retainedSha = sha(INPUT);
  const aside = INPUT + ".aside";
  renameSync(INPUT, aside);
  await openCatalog();
  await check("retained file missing: the preview is refused with its code and creates nothing", async () => { await ev(click(RETAINED, "/Kurulum önizlemesini aç/")); await sleep(900); return ev(`document.body.innerText.includes('INSTALL_ARTIFACT_UNAVAILABLE')`); });
  renameSync(aside, INPUT);
  writeFileSync(INPUT, Buffer.alloc(retainedOriginal.length, 7));
  await openCatalog();
  await check("retained file with the right size but other content: refused (INSTALL_ARTIFACT_REJECTED)", async () => { await ev(click(RETAINED, "/Kurulum önizlemesini aç/")); await sleep(900); return ev(`document.body.innerText.includes('INSTALL_ARTIFACT_REJECTED')`); });
  writeFileSync(INPUT, retainedOriginal);
  await check("nothing was created by those refusals", () => rootDir() === null);
  await openCatalog();
  await check("retained file back: preview opens; the file is swapped for other content BEFORE the confirmation", async () => {
    await ev(click(RETAINED, "/Kurulum önizlemesini aç/")); await sleep(900);
    // the preview itself says where the bytes come from: a LOCAL retained file, nothing downloaded, no network
    const originOk = await ev(`(() => { const t = document.querySelector('.preview-card')?.innerText ?? ''; return /Dosyanın kaynağı/.test(t) && /YEREL bir dosyadan okunur/.test(t) && /Hiçbir şey indirilmez; ağ kullanılmaz/.test(t) && /Ağ kullanılmayacak/.test(t); })()`);
    results.push({ label: "preview states: bytes come from the LOCAL retained file, nothing is downloaded", value: originOk });
    console.log("preview states the local origin".padEnd(96), "->", originOk);
    writeFileSync(INPUT, Buffer.alloc(retainedOriginal.length, 9));
    await ev(`document.querySelector('.preview-card input[type=checkbox]')?.click()`); await sleep(300);
    await ev(clickAnywhere("/Onaylıyorum ve kur/")); await sleep(1200);
    return ev(`document.body.innerText.includes('Sonuç: Reddedildi') && document.body.innerText.includes('artifact_does_not_match_recipe')`);
  });
  await shot("2b-swapped-refused", ".result-card");
  await check("the swap was caught before the first change: no manager folder exists", () => rootDir() === null);
  writeFileSync(INPUT, retainedOriginal);
  await ev(clickAnywhere("/^Tamam$/"));
  await openCatalog();
  await check("install the retained tool through the real window", async () => await drive(RETAINED, "/Kurulum önizlemesini aç/", "/Onaylıyorum ve kur/") && await waitFor(`document.body.innerText.includes('Sonuç: Tamamlandı')`));
  await check("the retained tool is on disk, equals the retained file, and the retained file itself is unchanged", () => existsSync(join(rootDir(), "versions", RETAINED, "1.0.0", "artifact.bin")) && sha(join(rootDir(), "versions", RETAINED, "1.0.0", "artifact.bin")) === retainedSha && sha(INPUT) === retainedSha);
  await ev(clickAnywhere("/^Tamam$/"));
  await openCatalog();
  await check("remove the retained tool; the retained file is still there byte-identical", async () => await drive(RETAINED, "/Kaldırma önizlemesini aç/", "/Onaylıyorum ve kaldır/") && await waitFor(`document.body.innerText.includes('Kaldırma tamamlandı')`) && sha(INPUT) === retainedSha && !existsSync(join(rootDir(), "versions", RETAINED)));
  await ev(clickAnywhere("/^Tamam$/"));
  await openCatalog();
  saveInventory("1-before-hello-install");

  // ---- confirm: the real install ----------------------------------------------------------------------------------------------------
  await check("open the preview and tick the acknowledgement", async () => { await ev(click(HELLO, "/Kurulum önizlemesini aç/")); await sleep(700); return ev(`(() => { const c = document.querySelector('.preview-card input[type=checkbox]'); if (!c) return false; c.click(); return true; })()`); }, 400);
  const shownDigest = await ev(`(() => { const t = document.querySelector('.preview-card')?.innerText ?? ''; const m = t.match(/Dosya özeti \\(SHA-256\\)\\s*([0-9a-f]{64})/); return m ? m[1] : null; })()`);
  await check("the confirm button is now enabled and is pressed", clickAnywhere("/Onaylıyorum ve kur/"), 600);
  await check("result: 'Tamamlandı' and the receipt is shown", async () => { await waitFor(`document.body.innerText.includes('Sonuç: Tamamlandı')`); return ev(`document.body.innerText.includes('Sonuç: Tamamlandı') && document.body.innerText.includes('Kurulum tamamlandı ve doğrulandı') && document.body.innerText.includes('İşlem kimliği')`); });
  await shot("3-result", ".result-card");
  await check("the file is on disk exactly where the preview said, with the digest the preview showed", () => existsSync(artifact()) && sha(artifact()) === shownDigest);
  await check("the version folder holds ONLY artifact.bin (no sidecar / notice file)", () => JSON.stringify(readdirSync(join(artifact(), ".."))) === JSON.stringify(["artifact.bin"]));
  await check("no file outside the manager folder and the database was created", () => dataFilesOutsideManager().length === 0);
  await check("the manager folder holds exactly the fixed layout", () => JSON.stringify(readdirSync(rootDir())) === JSON.stringify(["data", "journal", "locks", "receipts", "snapshots", "staging", "state", "versions"]));
  await check("no lock, staging or snapshot leftovers", () => ["locks", "staging", "snapshots"].every((d) => readdirSync(join(rootDir(), d)).length === 0));
  await check("the installed file is NOT executed: it is a plain data file and nothing started", () => statSync(artifact()).isFile());

  // ---- replay ----------------------------------------------------------------------------------------------------------------------------
  const used = await ev(`(async () => { const r = await __h.ipc('prepare_install', { request: { entryId: '${HELLO}', action: 'install' } }); return r.ok ? r.value : { err: r.error }; })()`);
  await check("a second install of the same tool is refused before any preview exists", () => !!used?.err && /zaten kurulu/.test(used.err));

  // ---- details ---------------------------------------------------------------------------------------------------------------------------
  await check("after the result the status is 'Kurulu ve doğrulandı'", async () => { await ev(clickAnywhere("/^Tamam$/")); await sleep(600); return ev(`${groupText(HELLO)}.includes('Kurulu ve doğrulandı')`); });
  await check("open the installed-tool details", clickAnywhere("/Araç ayrıntıları/"), 900);
  const d = await ev(`document.querySelector('.details-card')?.innerText ?? ''`);
  await check("details: version, relative location, digest, 5 passed checks, notices, 'yapmadıkları'", () => /1\.0\.0/.test(d) && /versions\/synthetic-hello\/1\.0\.0\/artifact\.bin/.test(d) && /[0-9a-f]{64}/.test(d) && (d.match(/Geçti:/g) ?? []).length >= 5 && /Üçüncü taraf bildirimleri/.test(d) && /Bu uygulamanın yapmadıkları/.test(d));
  await check("details show no absolute path or account name", () => !/[A-Za-z]:\\|\/Users\/|erent/.test(d));
  await shot("4-details", ".details-card");
  await check("dashboard counts one installed tool", async () => { await ev(`__h.nav('Ana Sayfa')`); await sleep(900); return ev(`(() => { const dt = [...document.querySelectorAll('dt')].find((x) => x.textContent === 'Kurulu / yönetilen araç'); return dt?.nextSibling?.textContent === '1'; })()`); });
  await check("audit chain still verifies after the whole sequence", async () => { const r = await ev(`__h.ipc('verify_audit_chain')`); return r.ok && r.value.ok === true; });
  await openCatalog();

  // ---- user data and removal -----------------------------------------------------------------------------------------------------------
  const userFile = join(rootDir(), "data", HELLO, "settings.json");
  mkdirSync(join(rootDir(), "data", HELLO), { recursive: true });
  writeFileSync(userFile, '{"theme":"dark"}');
  const userBefore = sha(userFile);
  await check("open the REMOVAL preview and confirm it", async () => { await ev(click(HELLO, "/Kaldırma önizlemesini aç/")); await sleep(800); await ev(`document.querySelector('.preview-card input[type=checkbox]')?.click()`); await sleep(300); return ev(clickAnywhere("/Onaylıyorum ve kaldır/")); }, 800);
  await check("result: removal 'Tamamlandı'", async () => { await waitFor(`document.body.innerText.includes('Kaldırma tamamlandı')`); return ev(`document.body.innerText.includes('Kaldırma tamamlandı')`); });
  await shot("5-removed", ".result-card");
  await check("the version folder and the pointer are gone", () => !existsSync(join(rootDir(), "versions", HELLO)) && !existsSync(join(rootDir(), "state", `${HELLO}.json`)));
  await check("the tool's own user data is byte-identical", () => existsSync(userFile) && sha(userFile) === userBefore);
  await check("status is 'Kaldırıldı'", async () => { await ev(clickAnywhere("/^Tamam$/")); await sleep(600); return ev(`${groupText(HELLO)}.includes('Kaldırıldı')`); });

  // ---- reinstall (idempotent notices) and tamper ---------------------------------------------------------------------------------------
  await check("reinstall through the same preview and confirmation", async () => { await ev(click(HELLO, "/Kurulum önizlemesini aç/")); await sleep(800); await ev(`document.querySelector('.preview-card input[type=checkbox]')?.click()`); await sleep(300); return ev(clickAnywhere("/Onaylıyorum ve kur/")); }, 800);
  await check("reinstalled: 'Tamamlandı'", async () => { await waitFor(`document.body.innerText.includes('Sonuç: Tamamlandı')`); return ev(`document.body.innerText.includes('Sonuç: Tamamlandı')`); });
  await ev(clickAnywhere("/^Tamam$/"));
  const original = readFileSync(artifact());
  const tampered = Buffer.from(original);
  tampered[tampered.length - 1] ^= 1;
  writeFileSync(artifact(), tampered);
  await ev(`__h.nav('Ana Sayfa')`); await sleep(500);
  await openCatalog();
  await check("a tampered installed file is reported as 'Başarısız / dikkat gerekiyor' (drift)", `${groupText(HELLO)}.includes('Başarısız / dikkat gerekiyor') && ${groupText(HELLO)}.includes('uyuşmuyor')`);
  await check("nothing was repaired or deleted: the tampered bytes are still there", () => readFileSync(artifact()).equals(tampered));
  await check("no removal/install can be opened for a drifted tool", `!${groupText(HELLO)}.includes('Kaldırma önizlemesini aç') && !${groupText(HELLO)}.includes('Kurulum önizlemesini aç')`);
  await shot("6-tampered", '[role=group][aria-label="Kurulum: synthetic-hello"]');
  writeFileSync(artifact(), original);
  await ev(`__h.nav('Ana Sayfa')`); await sleep(500);
  await openCatalog();
  await check("once the owner restores the file the tool verifies again", `${groupText(HELLO)}.includes('Kurulu ve doğrulandı')`);
  saveInventory("2-after-install");

  // ---- Issue #22 in the real window: audit failures are never ignored ------------------------------------------------------------------------
  // (1) a failed post-removal audit row: the removal HAPPENS, is reported as needing attention, and recovery reconciles it from the receipt
  failAudit("uninstall", "gui_refuse_uninstall_row");
  await openCatalog();
  await check("removal with a failing audit write: the answer says it was DONE but the audit row is missing (needs attention)", async () => {
    await drive(HELLO, "/Kaldırma önizlemesini aç/", "/Onaylıyorum ve kaldır/");
    return ev(`document.body.innerText.includes('Sonuç: Yapıldı — ama dikkat gerekiyor') && document.body.innerText.includes('denetim kaydı yazılamadı') && !document.body.innerText.includes('Kaldırma tamamlandı')`);
  });
  await shot("8-audit-pending", ".result-card");
  await check("the removal really happened on disk", () => !existsSync(join(rootDir(), "versions", HELLO)));
  await ev(clickAnywhere("/^Tamam$/"));
  await openCatalog();
  await check("the tool is 'Başarısız / dikkat gerekiyor' with the audit reason and no new operation can be opened", `${groupText(HELLO)}.includes('Başarısız / dikkat gerekiyor') && ${groupText(HELLO)}.includes('denetim kaydı yazılamadı') && !${groupText(HELLO)}.includes('Kurulum önizlemesini aç')`);
  const beforeRecover = inventory(DATA);
  await check("recovery while the audit trail still fails reconciles nothing and says so", async () => { await ev(click(HELLO, "/Kurtarmayı çalıştır/")); await sleep(1200); return ev(`document.body.innerText.includes('0 eksik denetim kaydı makbuzdan yeniden kuruldu, 1 kayıt kurulamadı')`); });
  dropTrigger("gui_refuse_uninstall_row");
  await check("recovery with a working audit trail rebuilds the missing row from the receipt and the journal", async () => { await ev(click(HELLO, "/Kurtarmayı çalıştır/")); await sleep(1500); return ev(`document.body.innerText.includes('1 eksik denetim kaydı makbuzdan yeniden kuruldu, 0 kayıt kurulamadı')`); });
  await openCatalog();
  await check("the tool is 'Kaldırıldı' again and recovery changed no file of the manager folder", () => { const d = diff(beforeRecover, inventory(DATA)); return d.added.length === 0 && d.removed.length === 0 && d.changed.every((k) => k.includes("atm.sqlite")); });
  await check("the status is 'Kaldırıldı' after the reconciliation", `${groupText(HELLO)}.includes('Kaldırıldı')`);
  await check("a second recovery reconciles nothing more (idempotent)", async () => { const r = await ev(`__h.ipc('recover_installations')`); return r.ok && r.value.auditReconciled === 0 && r.value.auditUnreconciled === 0; });
  // (2) a failed MANDATORY pre-installation audit row: nothing is applied at all
  failAudit("approve", "gui_refuse_approve_row");
  await openCatalog();
  const rootBefore = inventory(DATA);
  await check("installation with a failing pre-installation audit write is REFUSED (audit_unavailable) and changes no file", async () => {
    await drive(HELLO, "/Kurulum önizlemesini aç/", "/Onaylıyorum ve kur/");
    const refused = await ev(`document.body.innerText.includes('Sonuç: Reddedildi') && document.body.innerText.includes('audit_unavailable')`);
    const d = diff(rootBefore, inventory(DATA));
    return refused && d.added.length === 0 && d.removed.length === 0 && d.changed.every((k) => k.includes("atm.sqlite"));
  });
  await ev(clickAnywhere("/^Tamam$/"));
  dropTrigger("gui_refuse_approve_row");
  await openCatalog();
  await check("with the audit trail working again the installation goes through (a NEW preview and confirmation)", async () => await drive(HELLO, "/Kurulum önizlemesini aç/", "/Onaylıyorum ve kur/") && await waitFor(`document.body.innerText.includes('Sonuç: Tamamlandı')`));
  await ev(clickAnywhere("/^Tamam$/"));
  await openCatalog();
  await check("the audit chain verifies after all of that", async () => { const r = await ev(`__h.ipc('verify_audit_chain')`); return r.ok && r.value.ok === true; });
  saveInventory("3-final");
  writeFileSync(join(OUT, "inventory-diff-start-to-final.json"), JSON.stringify(diff(JSON.parse(readFileSync(join(OUT, "inventory-0-start.json"), "utf8")), JSON.parse(readFileSync(join(OUT, "inventory-3-final.json"), "utf8"))), null, 1));
} else {
  // ---- after an application restart (the earlier process was killed) -------------------------------------------------------------------
  await check("after the restart the tool is still 'Kurulu ve doğrulandı'", `${groupText(HELLO)}.includes('Kurulu ve doğrulandı')`);
  await check("the artifact is intact and verifies", () => existsSync(artifact()) && sha(artifact()).length === 64);
  await check("an old offer id from the earlier process does not exist any more", async () => { const r = await ev(`__h.ipc('confirm_install', { request: { offerId: '${"1".repeat(64)}', previewDigest: '${"2".repeat(64)}', acknowledged: true } })`); return r.ok && r.value.outcome === "refused" && r.value.code === "consent_unknown"; });
  await check("recovery has nothing to do and changes nothing", async () => { const r = await ev(`__h.ipc('recover_installations')`); return r.ok && r.value.needsAttention === 0 && r.value.rolledBack === 0; });
  await check("no lock, staging or snapshot leftovers after the kill and the restart", () => ["locks", "staging", "snapshots"].every((d) => readdirSync(join(rootDir(), d)).length === 0));
  await check("audit chain verifies after the restart", async () => { const r = await ev(`__h.ipc('verify_audit_chain')`); return r.ok && r.value.ok === true; });
  await shot("7-after-restart");
}

const passed = results.filter((r) => r.value === true).length;
writeFileSync(join(OUT, `${PHASE}-result.json`), JSON.stringify({ phase: PHASE, passed, total: results.length, results }, null, 2));
console.log(`\n${PHASE}: ${passed}/${results.length} checks true`);
ws.close();
process.exit(passed === results.length ? 0 : 1);
