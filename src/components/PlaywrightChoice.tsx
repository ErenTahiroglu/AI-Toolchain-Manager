// CATALOG-02 - "Playwright seçenekleri": a read-only comparison of the three Playwright candidates on the Catalog page. It reads the catalog entries
// the page already holds and the project list (the existing read-only `list_projects`); it saves, enables, installs, scans and connects to nothing.
// Its only action, "Matriste aç", hands ONE exact project + operating system + agent surface + variant to the Project Tool Workspace through the
// existing handoff, where the user alone may save an ordinary preference. Nothing is preselected, defaulted or substituted.
import { useId, useRef, useState } from "react";
import { api } from "../api";
import { compatLabel, platformLabel, surfaceLabel } from "../labels";
import {
  FAMILY_IDS, FUTURE_RUNTIME_NOTE, NEED_TARGET_MESSAGE, PLAYWRIGHT_INTRO, PLAYWRIGHT_VARIANTS, SUGGESTED_NOTE, isCompleteTarget, optionStateFor, recordNotes,
  type PlaywrightVariant,
} from "../playwrightChoice";
import type { CatalogEntry } from "../types";
import { TARGET_PLATFORMS, TARGET_SURFACES, acceptBlockReason, compatStateFor, type TargetRequest } from "../workspace";
import { ErrorNotice, useLoad } from "./common";

export type OpenInMatrix = (request: TargetRequest & { entryId: string }) => void;

export const hasPlaywrightFamily = (entries: readonly CatalogEntry[]) => entries.some((e) => FAMILY_IDS.includes(e.entryId));

function Option({
  variant, entries, target, targetName, onOpen,
}: {
  variant: PlaywrightVariant;
  entries: readonly CatalogEntry[];
  target: { alias: string; platform: string; surface: string };
  targetName: string;
  onOpen: ((variant: PlaywrightVariant) => void) | null;
}) {
  const uid = useId();
  const state = optionStateFor(entries, variant.entryId);
  const complete = isCompleteTarget(target);
  const notes = recordNotes(state);
  const compat = state.kind === "present" && complete ? compatStateFor(state.entry, target.platform, target.surface) : null;
  const block = state.kind === "present" && complete && compat !== null ? acceptBlockReason(state.entry, compat) : null;
  const where = complete ? `${targetName} · ${platformLabel(target.platform)} · ${surfaceLabel(target.surface)}` : null;
  return (
    <li className="pw-option">
      <h3 id={`${uid}-h`}>{variant.title}</h3>
      <p className="small">
        <code>{variant.entryId}</code>
        {variant.suggestedForCodingAgents && <> <span className="tag">Yayıncı belgelerinin başlangıç noktası (kodlama ajanları)</span></>}
      </p>
      {variant.suggestedForCodingAgents && <p className="small muted">{SUGGESTED_NOTE}</p>}
      <p>{variant.when}</p>
      <details>
        <summary>Dikkat edilecekler</summary>
        <p className="small">{variant.watch}</p>
      </details>
      {notes.length > 0 && <ul className="small" aria-label="Katalog kaydı hakkında">{notes.map((n) => <li key={n}>{n}</li>)}</ul>}
      {compat !== null && (
        <p className="small" aria-live="polite">
          Seçtiğiniz hedef için yayıncı beyanı: <strong>{compatLabel(compat)}</strong>.{" "}
          {block ? <>Matriste “kullanmak istiyorum” seçilemez: {block}</> : "Bu, test edildiği veya çalıştığı anlamına gelmez; yalnızca yayıncının kendi beyanıdır."}
        </p>
      )}
      {onOpen && state.kind === "present" && (
        <p className="actions">
          <button type="button" className="secondary" aria-describedby={`${uid}-h`} onClick={() => onOpen(variant)}>
            Matriste aç<span className="sr-only">: {variant.short}{where ? ` (${where})` : ""}</span>
          </button>
        </p>
      )}
    </li>
  );
}

export function PlaywrightChoice({ entries, onOpenInMatrix }: { entries: readonly CatalogEntry[]; onOpenInMatrix?: OpenInMatrix }) {
  const uid = useId();
  const projects = useLoad(api.listProjects);
  const [alias, setAlias] = useState("");
  const [platform, setPlatform] = useState("");
  const [surface, setSurface] = useState("");
  const [message, setMessage] = useState("");
  const aliasRef = useRef<HTMLSelectElement>(null);
  const platformRef = useRef<HTMLSelectElement>(null);
  const surfaceRef = useRef<HTMLSelectElement>(null);

  const list = projects.data ?? [];
  const effectiveAlias = list.some((p) => p.alias === alias) ? alias : "";
  const vanished = alias !== "" && effectiveAlias === "" && projects.data !== null;
  const target = { alias: effectiveAlias, platform, surface };
  const targetName = list.find((p) => p.alias === effectiveAlias)?.displayName ?? "";

  const open = (variant: PlaywrightVariant) => {
    if (!onOpenInMatrix) return;
    if (!isCompleteTarget(target)) {
      setMessage(NEED_TARGET_MESSAGE);
      (effectiveAlias === "" ? aliasRef : platform === "" ? platformRef : surfaceRef).current?.focus();
      return;
    }
    setMessage("");
    onOpenInMatrix({ alias: target.alias, platform: target.platform, surface: target.surface, entryId: variant.entryId });
  };

  return (
    <div className="panel" role="group" aria-labelledby={`${uid}-h`}>
      <h2 id={`${uid}-h`}>Playwright seçenekleri</h2>
      <p className="muted">{PLAYWRIGHT_INTRO}</p>
      {onOpenInMatrix && (
        <fieldset className="field">
          <legend>Hangi hedef için bakıyorsunuz? (isteğe bağlı; hiçbir şey kendiliğinden seçilmez)</legend>
          {projects.error && <ErrorNotice error={projects.error} onRetry={projects.reload} />}
          {projects.data !== null && list.length === 0 && (
            <p role="status" className="small">Henüz proje yok. Önce Projeler sayfasında bir proje ekleyin; sonra buradan hedef seçebilirsiniz.</p>
          )}
          {vanished && <p role="status" className="small bad-text">Seçtiğiniz proje artık listede yok; seçim temizlendi.</p>}
          <div className="field">
            <label htmlFor={`${uid}-p`}>Proje</label>
            <select id={`${uid}-p`} ref={aliasRef} value={effectiveAlias} onChange={(e) => { setAlias(e.target.value); setMessage(""); }}>
              <option value="">Proje seçin</option>
              {list.map((p) => <option key={p.alias} value={p.alias}>{p.displayName}{p.kind === "synthetic" ? " (örnek)" : ""}</option>)}
            </select>
          </div>
          <div className="field">
            <label htmlFor={`${uid}-o`}>İşletim sistemi</label>
            <select id={`${uid}-o`} ref={platformRef} value={platform} onChange={(e) => { setPlatform(e.target.value); setMessage(""); }}>
              <option value="">İşletim sistemi seçin</option>
              {TARGET_PLATFORMS.map((p) => <option key={p} value={p}>{platformLabel(p)}</option>)}
            </select>
          </div>
          <div className="field">
            <label htmlFor={`${uid}-s`}>Ajan yüzeyi</label>
            <select id={`${uid}-s`} ref={surfaceRef} value={surface} onChange={(e) => { setSurface(e.target.value); setMessage(""); }}>
              <option value="">Ajan yüzeyi seçin</option>
              {TARGET_SURFACES.map((s) => <option key={s} value={s}>{surfaceLabel(s)}</option>)}
            </select>
          </div>
          <p className="small muted">“Matriste aç” yalnızca Proje Araç Matrisi’ni bu hedefle ve bu seçenekle açar. Karar vermek, kaydetmek ve tercihi değiştirmek yalnızca matriste, sizin açık eyleminizle olur.</p>
        </fieldset>
      )}
      {message && <p role="status" className="bad-text">{message}</p>}
      <ul className="pw-options">
        {PLAYWRIGHT_VARIANTS.map((v) => (
          <Option key={v.entryId} variant={v} entries={entries} target={target} targetName={targetName} onOpen={onOpenInMatrix ? open : null} />
        ))}
      </ul>
      <details>
        <summary>Gelecekte ortak bir CLI çalışma zamanı (yalnızca öneri)</summary>
        <p className="small">{FUTURE_RUNTIME_NOTE}</p>
      </details>
    </div>
  );
}
