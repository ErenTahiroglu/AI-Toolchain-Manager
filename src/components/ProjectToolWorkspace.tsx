// PRODUCT-01 - Project Tool Workspace ("Proje Arac Matrisi"): for ONE project, ONE operating system and ONE agent surface, the user sees every
// catalog tool next to (a) what the catalog says, (b) the compatibility recorded for exactly that target, (c) the user's own recorded wish and
// (d) - kept apart - the application's record of the real state. The only things this view can do are to store or forget a WISH, through the
// existing typed commands setProjectPreference / resetProjectPreference. It installs, enables, verifies and reads nothing.
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { api, toAppError } from "../api";
import { compatLabel, formatTime, kindLabel, licenseLabel, metadataLabel, platformLabel, policyLabel, reviewLabel, scopeLabel, surfaceLabel } from "../labels";
import { observationFor, type ObservationView } from "../observation";
import type { AppError, CatalogEntry, EntryKind, Inventory, ProjectView } from "../types";
import {
  DECISION_VALUES, NO_FILTER, TARGET_PLATFORMS, TARGET_SURFACES, WORKSPACE_DECISION_HELP, WORKSPACE_DECISION_LABEL, buildRows, contextKey, filterRows,
  isFiltering, recordedInstallText, recordedVerificationText, resolveEntryFocus, resolveTarget, sameKey, summarize, targetCounts, workspaceDecisionLabel,
  type CompatFilter, type DecisionFilter, type DecisionValue, type MatrixHandoff, type TargetRequest, type WorkspaceFilter, type WorkspaceRow,
} from "../workspace";
import { ErrorNotice } from "./common";
import { ConfigPreview } from "./ConfigPreview";
import { ObservationSection } from "./ObservationSection";

export interface LoadState<T> {
  data: T | null;
  error: AppError | null;
  loading: boolean;
  reload: () => void;
}

const KINDS: EntryKind[] = ["executable_tool", "agent_plugin", "skill_collection", "prompt_source"];
const DEFAULT_DAYS = 7;
const clampDays = (n: number) => (Number.isFinite(n) ? Math.min(365, Math.max(1, Math.round(n))) : DEFAULT_DAYS);

function catalogStatus(entry: CatalogEntry): string {
  if (entry.removedAt !== null) return "Katalogdan kaldırıldı";
  return entry.enabled ? "Katalogda etkin" : "Katalogda kapalı";
}

function ToolRow({ row, project, platform, surface, observation, onChanged, onNotice, spot = 0 }: {
  row: WorkspaceRow; project: ProjectView; platform: string; surface: string; onChanged: () => void; onNotice: (text: string) => void;
  /** What the last stored discovery observation says about THIS row (see observationFor). Read-only; it is not part of the wish. */
  observation: ObservationView;
  /** > 0: this is the tool a recommendation pointed at. The row is marked in text, scrolled into view and focused once per request (the value is that request's nonce). */
  spot?: number;
}) {
  const uid = useId();
  const articleRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (spot > 0) {
      articleRef.current?.focus();
      articleRef.current?.scrollIntoView?.({ block: "center" });
    }
  }, [spot]);
  const { entry, pref, removed } = row;
  const [draft, setDraft] = useState<string>(pref?.decision ?? "");
  const [days, setDays] = useState(DEFAULT_DAYS);
  const [busy, setBusy] = useState<null | "save" | "reset">(null);
  const busyRef = useRef(false);
  const [error, setError] = useState<AppError | null>(null);

  // after a refresh the select follows what the application now holds
  useEffect(() => {
    setDraft(pref?.decision ?? "");
  }, [pref?.decision]);

  const key = { alias: project.alias, entryId: entry.entryId, agentSurface: surface, platformTarget: platform };
  const blockedAccept = row.acceptBlock;
  const decisionKnown = (DECISION_VALUES as readonly string[]).includes(draft);
  const unchanged = pref !== null && draft === pref.decision && draft !== "postponed";
  const canSave = !removed && decisionKnown && !unchanged && busy === null && !(draft === "accepted" && blockedAccept !== null);

  const save = async () => {
    if (busyRef.current || !canSave) return;
    busyRef.current = true;
    setBusy("save");
    setError(null);
    try {
      const saved = await api.setProjectPreference(key, draft, draft === "postponed" ? clampDays(days) : undefined);
      if (!sameKey(saved, key)) {
        setError({ code: "PREFERENCE_ANSWER_MISMATCH", message: "Uygulamanın yanıtı seçtiğiniz araç / ajan / işletim sistemi birleşimiyle eşleşmedi; kayıt doğrulanamadı. Listeyi yenileyip kontrol edin." });
      } else {
        onNotice(`${entry.displayName}: “${workspaceDecisionLabel(saved.decision)}” isteği kaydedildi. Yalnızca isteğiniz kaydedildi; hiçbir şey kurulmadı, doğrulanmadı veya bir ajanda etkinleştirilmedi.`);
      }
      onChanged();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  };

  const reset = async () => {
    if (busyRef.current || busy !== null || pref === null) return;
    busyRef.current = true;
    setBusy("reset");
    setError(null);
    try {
      await api.resetProjectPreference(key);
      onNotice(`${entry.displayName}: kayıtlı karar sıfırlandı. Hiçbir şey kurulmadı, kaldırılmadı veya değiştirilmedi.`);
      onChanged();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  };

  const installText = recordedInstallText(pref?.actualInstallState ?? entry.installState);
  const verifyText = recordedVerificationText(pref?.actualVerificationState ?? entry.verificationState);
  const headId = `${uid}-h`;

  return (
    <li className={`ws-row${removed ? " removed" : ""}${spot > 0 ? " ws-spot" : ""}`}>
      <article ref={articleRef} tabIndex={-1} aria-labelledby={headId} aria-busy={busy !== null}>
        <h3 id={headId}>{entry.displayName}</h3>
        <p className="small">
          {spot > 0 && <><span className="tag">Öneriden gelen araç</span>{" "}</>}
          <span className="tag">{kindLabel(entry.kind)}</span>{" "}
          <span className="tag">{catalogStatus(entry)}</span>{" "}
          <span className="tag">{reviewLabel(entry.reviewState)}</span>{" "}
          {removed && <span className="tag">katalogdan kaldırıldı</span>}
        </p>
        <p>{entry.purpose}</p>

        <div className="ws-grid">
          <dl className="facts small" aria-label={`${entry.displayName}: katalog bilgisi`}>
            <dt>Kaynak</dt>
            <dd>{entry.repoUrl}{entry.sourcePath ? ` (klasör: ${entry.sourcePath})` : ""} <span className="muted">· {metadataLabel(entry.metadataState)}</span></dd>
            <dt>Lisans</dt>
            <dd>{licenseLabel(entry.licenseState)}{entry.licenseSpdx ? `: ${entry.licenseSpdx}` : ""}</dd>
            <dt>Çalıştırma ilkesi</dt>
            <dd>{policyLabel(entry.executionPolicy)}</dd>
            <dt>Uyumluluk ({platformLabel(platform)} · {surfaceLabel(surface)})</dt>
            <dd>
              <strong>{compatLabel(row.compat)}</strong>
              {row.evidence ? <span className="muted"> · {row.evidence.limitation}</span> : <span className="muted"> · Bu birleşim için kayıtlı kanıt yok; “desteklenmiyor” değil, <em>bilinmiyor</em> sayılır.</span>}
            </dd>
          </dl>

          <dl className="facts small" aria-label={`${entry.displayName}: istek ve gerçek durum`}>
            <dt>İsteğiniz</dt>
            <dd>
              {pref ? (
                <>
                  <strong>{workspaceDecisionLabel(pref.decision)}</strong>
                  <span className="muted"> · {formatTime(pref.decidedAt)} · katalog sürümü {pref.catalogRevision}</span>
                  {pref.remindAt && <div className="muted">Hatırlatma: {formatTime(pref.remindAt)}</div>}
                  {pref.materialChange && <div className="bad-text">⚠ Aracın yetenekleri veya uyumluluğu bu karardan sonra önemli biçimde değişti; kararı gözden geçirin.</div>}
                  {!pref.materialChange && pref.revisionChanged && <div className="muted">Katalog sürümü değişti; bu tek başına kararınızı geçersiz kılmaz.</div>}
                </>
              ) : (
                <strong>Henüz karar yok</strong>
              )}
            </dd>
            <dt>Gerçek durum (ayrı; bu istekle değişmez)</dt>
            <dd>
              <div>Kurulum: {installText}</div>
              <div>Doğrulama: {verifyText}</div>
              {pref && <div>Kapsam: {scopeLabel(pref.observedScope)}</div>}
              <div>Ajanda etkin mi: Bilinmiyor — bu uygulama ajan ayarlarına bakmaz.</div>
              <div className="muted">Katalog durumu (“{catalogStatus(entry)}”) yalnızca bir katalog işaretidir; kurulum veya ajan durumu değildir.</div>
            </dd>
          </dl>

          <ObservationSection view={observation} entryName={entry.displayName} />
        </div>

        <details>
          <summary>Sınırlamalar ve bilinmeyenler ({entry.limitations.length})</summary>
          <ul className="small">{entry.limitations.map((l) => <li key={l}>{l}</li>)}</ul>
        </details>

        <ConfigPreview entry={entry} alias={project.alias} projectName={project.displayName} platform={platform} surface={surface} />

        {removed ? (
          <p className="small" role="note">Bu kayıt katalogdan kaldırıldı: yeni karar kaydedilemez. Eski kararınız yalnızca görülebilir veya sıfırlanabilir.</p>
        ) : (
          <div className="inline-form">
            <div className="field">
              <label htmlFor={`${uid}-d`}>Karar <span className="sr-only">({entry.displayName})</span></label>
              <select id={`${uid}-d`} value={draft} disabled={busy !== null} onChange={(e) => { setDraft(e.target.value); setError(null); }} aria-describedby={`${uid}-help`}>
                <option value="">Karar seçin</option>
                {DECISION_VALUES.map((d: DecisionValue) => (
                  <option key={d} value={d} disabled={d === "accepted" && blockedAccept !== null}>{WORKSPACE_DECISION_LABEL[d]}{d === "accepted" && blockedAccept !== null ? " (şu an seçilemez)" : ""}</option>
                ))}
              </select>
              <div id={`${uid}-help`} className="small muted">
                {decisionKnown ? WORKSPACE_DECISION_HELP[draft as DecisionValue] : "Seçtiğiniz karar yalnızca bu proje, araç, işletim sistemi ve ajan yüzeyi birleşimi için kaydedilir."}
              </div>
              {blockedAccept !== null && <div className="small bad-text">⚠ {blockedAccept}</div>}
            </div>
            {draft === "postponed" && (
              <div className="field">
                <label htmlFor={`${uid}-n`}>Kaç gün sonra hatırlat <span className="sr-only">({entry.displayName})</span></label>
                <input id={`${uid}-n`} type="number" min={1} max={365} value={days} disabled={busy !== null} onChange={(e) => setDays(Number(e.target.value))} />
              </div>
            )}
          </div>
        )}

        <p className="actions">
          {!removed && (
            <button type="button" onClick={save} disabled={!canSave} aria-disabled={!canSave}>
              {busy === "save" ? "Kaydediliyor…" : "Yalnızca tercihimi kaydet"}{" "}<span className="sr-only">({entry.displayName}; kurulum yapmaz)</span>
            </button>
          )}
          {pref && (
            <button type="button" className="secondary" onClick={reset} disabled={busy !== null} aria-disabled={busy !== null}>
              {busy === "reset" ? "Sıfırlanıyor…" : removed ? "Eski kararı sıfırla" : "Kararı sıfırla"}{" "}<span className="sr-only">({entry.displayName})</span>
            </button>
          )}
        </p>
        {error && <ErrorNotice error={error} />}
      </article>
    </li>
  );
}

export function ProjectToolWorkspace({ projects, catalog, inventory, target, onTarget, focusNonce = 0, focus = null }: {
  projects: LoadState<ProjectView[]>;
  catalog: LoadState<CatalogEntry[]>;
  /** The stored (database-only) inventory read. Never a scan; a failed read is shown as unknown, never as "not found". */
  inventory: LoadState<Inventory>;
  /** The REQUESTED target (project alias, operating system, agent surface). It is resolved against what exists; see resolveTarget. */
  target: TargetRequest;
  onTarget: (t: TargetRequest) => void;
  /** Increase to move focus to the panel heading (used when another part of the page sends the user here). */
  focusNonce?: number;
  /**
   * A recommendation asked for this tool in this target (see MatrixHandoff). It is navigation only: the row is brought into view and nothing is
   * decided, saved, evaluated or installed. It lapses as soon as the user picks another target, and it never selects a different tool.
   */
  focus?: MatrixHandoff | null;
}) {
  const [filter, setFilter] = useState<WorkspaceFilter>(NO_FILTER);
  // The confirmation of the last save/reset belongs to the target it was made for and is only ever shown there (never carried to another target).
  const [notice, setNotice] = useState<{ ctx: string; text: string } | null>(null);
  const ids = useId();
  // the request still waiting for data / a cleared filter; the row it settled on; and what the user should be told - the last two belong to one target
  const [wanted, setWanted] = useState<MatrixHandoff | null>(null);
  const [spot, setSpot] = useState<{ ctx: string; entryId: string; nonce: number } | null>(null);
  const [focusNote, setFocusNote] = useState<{ ctx: string; lead: string; notes: string[] } | null>(null);
  const clearedFor = useRef(0);

  const titleRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (focusNonce > 0) {
      titleRef.current?.focus();
      titleRef.current?.scrollIntoView?.({ block: "start" });
    }
  }, [focusNonce]);

  const list = projects.data ?? [];
  const resolved = useMemo(() => resolveTarget(target, projects.data), [target, projects.data]);
  const { project, platform, surface } = resolved;
  const nav = useMemo(() => (project ? targetCounts(project) : null), [project]);
  const choose = (patch: Partial<TargetRequest>) => project && onTarget({ alias: project.alias, platform, surface, ...patch });
  const entries = catalog.data;

  const built = useMemo(() => (project && entries ? buildRows(entries, project, platform, surface) : null), [project, entries, platform, surface]);
  const visible = useMemo(() => (built ? filterRows(built.rows, filter) : []), [built, filter]);
  const summary = useMemo(() => summarize(visible), [visible]);
  const all = useMemo(() => (built ? summarize(built.rows) : null), [built]);

  const waiting = (projects.loading && !projects.data) || (catalog.loading && !catalog.data);
  const refreshing = projects.loading || catalog.loading || inventory.loading;
  // everything below re-reads the application's own stored records; none of it scans the computer
  const refresh = () => { projects.reload(); catalog.reload(); inventory.reload(); };
  const afterSave = () => { projects.reload(); inventory.reload(); };
  const set = (patch: Partial<WorkspaceFilter>) => setFilter((f) => ({ ...f, ...patch }));
  const ctx = project ? contextKey(project.alias, platform, surface) : "";

  useEffect(() => {
    setWanted(focus && focus.entryId !== null ? focus : null);
    setSpot(null);
    setFocusNote(null);
    clearedFor.current = 0;
  }, [focus]);

  // the mark and the note describe one target; once the user is elsewhere they are gone, and they do not come back when the user returns
  useEffect(() => {
    if (spot && spot.ctx !== ctx) setSpot(null);
    if (focusNote && focusNote.ctx !== ctx) setFocusNote(null);
  }, [ctx, spot, focusNote]);

  useEffect(() => {
    if (!wanted || wanted.entryId === null) return;
    // the user moved to another target meanwhile: the request no longer applies and must not land on a different project
    if (target.alias !== wanted.alias || target.platform !== wanted.platform || target.surface !== wanted.surface) {
      setWanted(null);
      return;
    }
    if (waiting) return;
    if (!project || !entries || !built) {
      setFocusNote({ ctx, lead: "Önerilen araç satırı vurgulanamadı.", notes: ["Proje veya katalog bilgisi yüklenemedi ya da boş; başka bir araç seçilmedi."] });
      setWanted(null);
      return;
    }
    const out = resolveEntryFocus(wanted.entryId, resolved, entries, built.rows, filter);
    if (out.kind === "unavailable") {
      setFocusNote({ ctx, lead: "Önerilen araç satırı vurgulanamadı.", notes: out.notes });
      setWanted(null);
      return;
    }
    if (out.needsFilterClear) {
      clearedFor.current = wanted.nonce;
      setFilter(NO_FILTER); // this effect runs again once the row is visible
      return;
    }
    const entryName = entries.find((e) => e.entryId === wanted.entryId)?.displayName ?? wanted.entryId;
    setSpot({ ctx, entryId: wanted.entryId, nonce: wanted.nonce });
    setFocusNote({
      ctx,
      lead: `Öneriden açıldı: ${entryName}. Satır işaretlendi; henüz karar vermediniz ve hiçbir şey kaydedilmedi, değerlendirilmedi veya kurulmadı.`,
      notes: clearedFor.current === wanted.nonce ? ["Araç ve karar süzgeçleri bu aracı gizlediği için temizlendi.", ...out.notes] : out.notes,
    });
    setWanted(null);
  }, [wanted, target, waiting, project, entries, built, resolved, filter, ctx]);

  return (
    <section className="panel" aria-labelledby={`${ids}-title`} aria-busy={refreshing}>
      <h2 id={`${ids}-title`} tabIndex={-1} ref={titleRef}>Proje Araç Matrisi</h2>
      <p className="muted small">
        Bir proje, işletim sistemi ve ajan yüzeyi seçin; her araç için <strong>isteğinizi</strong> kaydedin. Bu bir <strong>planlama ekranıdır</strong>: hiçbir şey kurmaz,
        çalıştırmaz, doğrulamaz veya etkinleştirmez; ajan ayarlarına ve dosyalarınıza dokunmaz ve bilerek ağ bağlantısı başlatmaz (WebView çalışma ortamının ayrı ağ trafiği bunun dışındadır; açık sorun #25). “Gerçek durum” bölümü, bu ekranın bilgisayarı denetlediği
        anlamına gelmez; uygulamanın kendi kaydını gösterir. Kurulum durumunun ayrıntısı Araç Kütüphanesi sayfasındadır.
      </p>

      {projects.error && (
        <>
          <ErrorNotice error={projects.error} onRetry={projects.reload} />
          {projects.data && <p className="small bad-text">⚠ Proje listesi yenilenemedi; aşağıda gösterilenler eski olabilir.</p>}
        </>
      )}
      {catalog.error && (
        <>
          <ErrorNotice error={catalog.error} onRetry={catalog.reload} />
          {catalog.data && <p className="small bad-text">⚠ Katalog yenilenemedi; aşağıda gösterilenler eski olabilir.</p>}
        </>
      )}
      {inventory.error && (
        <>
          <ErrorNotice error={inventory.error} onRetry={inventory.reload} />
          <p className="small bad-text">
            ⚠ Kayıtlı envanter okunamadı. {inventory.data ? "Araç satırlarındaki gözlemler önceki başarılı okumadan ve eski olabilir." : "Gözlemler bilinmiyor; bu, “taranmadı” veya “bulunamadı” anlamına gelmez."}
          </p>
        </>
      )}
      {waiting && <p role="status">Yükleniyor…</p>}
      <div role="status" className="small ws-focus-note">
        {focusNote && focusNote.ctx === ctx && (
          <>
            <p>{focusNote.lead}</p>
            {focusNote.notes.map((n) => <p key={n} className="bad-text">⚠ {n}</p>)}
          </>
        )}
      </div>

      {projects.data && list.length === 0 && <p className="muted">Henüz proje kaydı yok. Aşağıdan bir proje kaydı ekleyebilirsiniz.</p>}

      {project && (
        <>
          <div className="row">
            <div className="field">
              <label htmlFor={`${ids}-p`}>Proje</label>
              <select id={`${ids}-p`} value={project.alias} onChange={(e) => choose({ alias: e.target.value })}>
                {list.map((p) => <option key={p.alias} value={p.alias}>{p.displayName}{p.kind === "synthetic" ? " (örnek)" : ""}</option>)}
              </select>
            </div>
            <div className="field">
              <label htmlFor={`${ids}-o`}>İşletim sistemi</label>
              <select id={`${ids}-o`} value={platform} onChange={(e) => choose({ platform: e.target.value })}>
                {TARGET_PLATFORMS.map((p) => <option key={p} value={p}>{platformLabel(p)}</option>)}
              </select>
            </div>
            <div className="field">
              <label htmlFor={`${ids}-s`}>Ajan yüzeyi</label>
              <select id={`${ids}-s`} value={surface} onChange={(e) => choose({ surface: e.target.value })}>
                {TARGET_SURFACES.map((s) => <option key={s} value={s}>{surfaceLabel(s)}</option>)}
              </select>
            </div>
          </div>
          <p className="small">
            Hedef: <strong>{project.displayName}</strong> · {platformLabel(platform)} · {surfaceLabel(surface)}. Kararlar yalnızca bu birleşim için geçerlidir; başka bir proje,
            işletim sistemi veya ajan yüzeyi için ayrıca kaydedilir.{" "}
            <button type="button" className="secondary" onClick={refresh} disabled={refreshing}>Listeyi yenile</button>
          </p>
          {resolved.notes.length > 0 && (
            <div role="note" className="small bad-text">{resolved.notes.map((n) => <p key={n}>⚠ {n}</p>)}</div>
          )}
          {nav && (
            <div role="group" aria-labelledby={`${ids}-nav`} className="ws-nav">
              <p id={`${ids}-nav`} className="small">
                <strong>Bu projedeki kayıtlı kararlar</strong> (yalnızca sayım; bir hedefi açmak için seçin — hiçbir şey kurulmaz veya değiştirilmez):
              </p>
              {nav.total === 0 ? (
                <p className="small muted">Bu projede henüz kayıtlı karar yok.</p>
              ) : (
                <ul className="ws-nav-list">
                  {nav.targets.map((t) => {
                    const current = t.platform === platform && t.surface === surface;
                    return (
                      <li key={`${t.platform}-${t.surface}`}>
                        <button type="button" className="secondary" aria-current={current ? "true" : undefined} onClick={() => choose({ platform: t.platform, surface: t.surface })}>
                          {platformLabel(t.platform)} · {surfaceLabel(t.surface)}: {t.count} karar{t.removed > 0 ? ` (${t.removed} katalogdan kaldırılmış kayıt)` : ""}{current ? " — şu an açık" : ""}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              )}
              {nav.unreachable > 0 && (
                <p className="small bad-text">⚠ {nav.unreachable} karar, bu sürümün tanımadığı bir işletim sistemi veya ajan yüzeyine ait olduğu için bu ekrandan seçilemiyor; silinmedi ve değiştirilmedi.</p>
              )}
            </div>
          )}
        </>
      )}

      {project && built && all && (
        <>
          <p role="status" className="small ok-text ws-notice">{notice && notice.ctx === ctx ? `✓ ${notice.text}` : ""}</p>

          <div role="search" aria-label="Araçları ara ve süz" className="row">
            <div className="field">
              <label htmlFor={`${ids}-q`}>Araç ara</label>
              <input id={`${ids}-q`} type="text" autoComplete="off" spellCheck={false} value={filter.query} onChange={(e) => set({ query: e.target.value })} />
            </div>
            <div className="field">
              <label htmlFor={`${ids}-k`}>Tür</label>
              <select id={`${ids}-k`} value={filter.kind} onChange={(e) => set({ kind: e.target.value as "all" | EntryKind })}>
                <option value="all">Tümü</option>
                {KINDS.map((k) => <option key={k} value={k}>{kindLabel(k)}</option>)}
              </select>
            </div>
            <div className="field">
              <label htmlFor={`${ids}-c`}>Karar durumu</label>
              <select id={`${ids}-c`} value={filter.decision} onChange={(e) => set({ decision: e.target.value as DecisionFilter })}>
                <option value="all">Tümü</option>
                <option value="undecided">Karar yok</option>
                {DECISION_VALUES.map((d) => <option key={d} value={d}>{WORKSPACE_DECISION_LABEL[d]}</option>)}
              </select>
            </div>
            <div className="field">
              <label htmlFor={`${ids}-u`}>Uyumluluk</label>
              <select id={`${ids}-u`} value={filter.compat} onChange={(e) => set({ compat: e.target.value as CompatFilter })}>
                <option value="all">Tümü</option>
                <option value="declared">Beyan var (üretici veya test)</option>
                <option value="unknown">Bilinmiyor</option>
                <option value="unsupported">Desteklenmiyor</option>
              </select>
            </div>
          </div>

          <div role="status" className="ws-summary small">
            <p>
              <strong>{visible.length}</strong> / {built.rows.length} araç gösteriliyor.{" "}
              İsteğiniz: {summary.accepted} kullanmak istiyorum · {summary.postponed} ertelendi · {summary.rejected} reddedildi · {summary.disabled} kapalı tutulacak · {summary.undecided} karar yok
              {summary.unrecognised > 0 ? ` · ${summary.unrecognised} tanınmayan karar` : ""}.
            </p>
            <p>
              Gerçek durum (uygulamanın kaydı, ayrı): {summary.recordedNotInstalled} kurulu değil / yönetilmiyor · {summary.recordedNotVerified} doğrulanmadı
              {summary.recordedInstalled > 0 ? ` · ${summary.recordedInstalled} kurulu olarak kaydedilmiş` : ""}
              {summary.recordedVerified > 0 ? ` · ${summary.recordedVerified} doğrulanmış olarak kaydedilmiş` : ""}
              {summary.recordedUnrecognised > 0 ? ` · ${summary.recordedUnrecognised} tanınmayan durum (başarı sayılmaz)` : ""}.
              {summary.materialChange > 0 ? ` Karardan sonra önemli biçimde değişen: ${summary.materialChange}.` : ""}
            </p>
          </div>

          {built.orphanPrefs.length > 0 && (
            <p className="small bad-text" role="note">⚠ Bu hedefte katalog listesinde bulunamayan {built.orphanPrefs.length} kayıtlı karar var; gösterilemiyor.</p>
          )}

          {built.rows.length === 0 && (
            <p className="muted">Katalogda hiç kayıt yok. Araç Kütüphanesi sayfasından GitHub deposu adayı ekleyebilirsiniz; ekleme yalnızca kayıt oluşturur.</p>
          )}
          {built.rows.length > 0 && visible.length === 0 && (
            <p>
              Bu süzgeçle eşleşen araç yok.{" "}
              {isFiltering(filter) && <button type="button" className="secondary" onClick={() => setFilter(NO_FILTER)}>Süzgeçleri temizle</button>}
            </p>
          )}

          <ul className="ws-list" aria-label="Araçlar">
            {visible.map((row) => (
              <ToolRow
                key={`${ctx}\u0000${row.entry.entryId}`} row={row} project={project} platform={platform} surface={surface} onChanged={afterSave} onNotice={(text) => setNotice({ ctx, text })}
                observation={observationFor(row, { alias: project.alias, platform, surface }, inventory)}
                spot={spot && spot.ctx === ctx && spot.entryId === row.entry.entryId ? spot.nonce : 0}
              />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
