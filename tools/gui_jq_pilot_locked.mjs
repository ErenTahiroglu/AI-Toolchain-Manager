// Phase 3D-4 (B2) GUI evidence driver: the REAL application window of a debug build made with `--features jq-pilot` (never a release build),
// started with an ISOLATED data directory. It shows that the jq row exists, is listed as "Doğrulama tamamlanmadı" with the reason that the
// installation is not enabled in this build, offers no install button, and that forged IPC calls can neither open a preview nor confirm anything.
// It reads the file system of the isolated directory before and after (tools/fs_inventory.mjs) and proves nothing but the application's own database
// files changed: no manager folder, no input folder, no file. It never provides, opens or runs jq; there is no jq file anywhere in this run.
//
// Environment: ATM_DATA_DIR (the isolated directory the application uses), EVIDENCE_DIR (screenshots and result.json are written here).
// tools/gui_jq_pilot_locked.ps1 starts and stops the application.
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { diff, inventory } from "./fs_inventory.mjs";

const DATA = process.env.ATM_DATA_DIR;
const OUT = process.env.EVIDENCE_DIR;
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
  if (selector) { await ev(`document.querySelector(${JSON.stringify(selector)})?.scrollIntoView({ block: 'center' })`); await sleep(250); }
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, `${name}.png`), Buffer.from(r.result.data, "base64"));
};
const results = [];
const check = async (label, expr, wait = 200) => {
  const v = typeof expr === "function" ? await expr() : await ev(expr);
  const shown = typeof v === "string" ? v : JSON.stringify(v);
  console.log(label.padEnd(100), "->", shown.length > 150 ? shown.slice(0, 150) + "…" : shown);
  results.push({ label, value: v });
  await sleep(wait);
  return v;
};
const waitFor = async (expr, timeout = 15000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < timeout) {
    if ((await ev(expr)) === true) return true;
    await sleep(200);
  }
  return false;
};

await ev(`window.__h = {
  group(tool) { return document.querySelector('[role=group][aria-label="Kurulum: ' + tool + '"]'); },
  nav(l) { [...document.querySelectorAll('nav button')].find((b) => b.textContent.trim() === l).click(); },
  async ipc(cmd, args) { try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(cmd, args) }; } catch (e) { return { ok: false, error: typeof e === 'string' ? e : JSON.stringify(e) }; } },
}`);
const groupText = (tool) => `(__h.group('${tool}') ? __h.group('${tool}').innerText : '')`;

await sleep(1500);
const before = inventory(DATA);
writeFileSync(join(OUT, "inventory-0-start.json"), JSON.stringify(before, null, 1));
await ev(`__h.nav('Araç Kütüphanesi')`);
await waitFor(`!!__h.group('jq')`);
await sleep(500);
await shot("1-catalog-jq-locked", "[role=group][aria-label='Kurulum: jq']");

await check("the jq row has an install panel (the pilot build seeds the catalog row)", `!!__h.group('jq')`);
await check("only the ordinary 13 entries and jq have a panel (no synthetic tool in this build)", `document.querySelectorAll('[role=group][aria-label^="Kurulum: "]').length === 14 && ![...document.querySelectorAll('[role=group][aria-label^="Kurulum: synthetic"]')].length`);
await check("jq: 'Doğrulama tamamlanmadı'", `${groupText("jq")}.includes('Doğrulama tamamlanmadı')`);
await check("jq: the reason says the installation is not enabled in this build", `/etkinleştirilmedi/.test(${groupText("jq")})`);
await check("jq: no install button, no remove button", `!/Kurulum önizlemesini aç|Kaldırma önizlemesini aç/.test(${groupText("jq")})`);
const text = await ev(groupText("jq"));
writeFileSync(join(OUT, "jq-panel-text.txt"), String(text));
await check("forged IPC: prepare_install(jq, install) is refused", async () => { const r = await ev(`__h.ipc('prepare_install', { request: { entryId: 'jq', action: 'install' } })`); return r.ok === false && /INSTALL_NOT_AVAILABLE/.test(r.error); });
await check("forged IPC: prepare_install(jq, uninstall) is refused", async () => { const r = await ev(`__h.ipc('prepare_install', { request: { entryId: 'jq', action: 'uninstall' } })`); return r.ok === false; });
await check("forged IPC: a request carrying a path / digest / unlock flag is rejected by the IPC layer", async () => {
  const bad = [{ path: "C:\\\\x\\\\jq.exe" }, { sha256: "00" }, { allowRealInstall: true }, { unlock: "x" }];
  for (const extra of bad) {
    const r = await ev(`__h.ipc('prepare_install', { request: { entryId: 'jq', action: 'install', ...${JSON.stringify(extra)} } })`);
    if (r.ok) return false;
  }
  return true;
});
await check("forged IPC: confirm_install of an unknown offer is refused and nothing happens", async () => { const r = await ev(`__h.ipc('confirm_install', { request: { offerId: '${"a".repeat(64)}', previewDigest: '${"b".repeat(64)}', acknowledged: true } })`); return r.ok && r.value.outcome === "refused"; });
await check("the installed-details of jq show nothing installed", async () => { const r = await ev(`__h.ipc('get_installed_details', { entryId: 'jq' })`); return r.ok && !r.value.receipt && r.value.state.status === "verification_incomplete" && r.value.state.canInstall === false; });
await sleep(800);

// ---- the file system: nothing but the application's own database files changed; no manager folder and no input folder was created -------------
const after = inventory(DATA);
writeFileSync(join(OUT, "inventory-1-final.json"), JSON.stringify(after, null, 1));
const d = diff(before, after);
writeFileSync(join(OUT, "inventory-diff.json"), JSON.stringify(d, null, 1));
await check("no manager folder (atm-managed) was created", () => !existsSync(join(DATA, "atm-managed")));
await check("no input folder (jq-pilot-input) was created: the application reads no retained file while locked", () => !existsSync(join(DATA, "jq-pilot-input")));
await check("the only changed files are the application's own SQLite files", () => [...(d.added ?? []), ...(d.removed ?? []), ...(d.changed ?? [])].every((k) => /^atm\.sqlite/.test(k) || /^backups\//.test(k) || /^(crash|diagnostic)/i.test(k) || k === "backups/"), 0);
await check("the catalog row jq exists once, unreviewed, manual-only, install_state not_managed", () => {
  const db = new DatabaseSync(join(DATA, "atm.sqlite"), { readOnly: true });
  const rows = db.prepare("SELECT entry_id, review_state, execution_policy, install_state, verification_state, license_state FROM catalog_entries WHERE entry_id = 'jq'").all();
  db.close();
  return rows.length === 1 && rows[0].review_state === "unreviewed" && rows[0].execution_policy === "manual-only" && rows[0].install_state === "not_managed" && rows[0].verification_state === "unknown";
});
await check("the audit trail holds no install or approve row (nothing was confirmed)", () => {
  const db = new DatabaseSync(join(DATA, "atm.sqlite"), { readOnly: true });
  const n = db.prepare("SELECT count(*) AS n FROM audit_events WHERE json_extract(record_json, '$.eventType') IN ('install', 'uninstall') OR (json_extract(record_json, '$.eventType') = 'approve' AND json_extract(record_json, '$.outcome') = 'passed')").get().n;
  db.close();
  return n === 0;
});

const failed = results.filter((r) => r.value !== true);
writeFileSync(join(OUT, "result.json"), JSON.stringify({ kind: "gui-jq-pilot-locked", total: results.length, passed: results.length - failed.length, failed: failed.map((f) => f.label), results }, null, 1));
console.log(`\n${results.length - failed.length}/${results.length} checks true`);
ws.close();
process.exit(failed.length === 0 ? 0 : 1);
