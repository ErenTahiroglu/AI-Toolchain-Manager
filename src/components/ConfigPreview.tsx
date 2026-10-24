// CONFIG-01 - "Yapılandırma önizlemesi": a read-only preview of what a project's MCP configuration FILE would look like if one reviewed declaration were added.
// It lives inside one matrix row, so the project, operating system, agent surface and catalog entry are exactly the row's - nothing is chosen for the user and
// nothing is defaulted. It reads no file (the user pastes text or picks "new file"), writes no file, calls no IPC command, uses no clipboard, no network and no
// storage, and offers no "apply". The decision itself stays in the matrix; this is not a preference-writing surface. All logic is in src/configPlan.ts.
import { useEffect, useId, useRef, useState } from "react";
import { platformLabel, surfaceLabel } from "../labels";
import { REVIEWED_DECLARATIONS, type ReviewedDeclaration } from "../configDeclarations";
import { isMcpEntry, isPlanStale, planConfigPreview, type PlanRequest, type PreviewPlan } from "../configPlan";
import type { CatalogEntry } from "../types";

type Mode = "" | "new" | "text";

function PlanView({ plan, projectName, entryName }: { plan: PreviewPlan; projectName: string; entryName: string }) {
  const uid = useId();
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    ref.current?.focus(); // the result is announced and reachable at once; moving focus does not change anything on screen but the focus ring
  }, [plan]);
  const ready = plan.status === "PREVIEW_ONLY_NOT_APPLIED";
  return (
    <section ref={ref} tabIndex={-1} role="region" aria-labelledby={`${uid}-h`} className="cfg-result">
      <h4 id={`${uid}-h`}>
        <span className="tag">ÖNİZLEME — UYGULANMADI</span> <span className={ready ? "" : "bad-text"}>{plan.statusLabel}</span> <code>{plan.status}</code>
      </h4>
      <p role="status"><strong>{plan.notice}</strong></p>
      <dl className="facts small" aria-label={`${entryName}: önizleme hedefi`}>
        <dt>Proje</dt><dd>{projectName} <span className="muted">({plan.identity.alias})</span></dd>
        <dt>İşletim sistemi</dt><dd>{platformLabel(plan.identity.platform)}</dd>
        <dt>Ajan yüzeyi</dt><dd>{surfaceLabel(plan.identity.surface)}</dd>
        <dt>Araç</dt><dd>{entryName} <code>{plan.identity.entryId}</code></dd>
        <dt>Dosya (projenin kökünden göreli)</dt><dd>{plan.relativePath ? <code>{plan.relativePath}</code> : "—"}</dd>
        <dt>Kapsam</dt><dd>{plan.scope ?? "—"}</dd>
        {plan.declaration && (<><dt>Onaylı bildirim</dt><dd>sunucu adı <code>{plan.declaration.serverName}</code> · gözden geçirme {plan.declaration.reviewId} · kanıt {plan.declaration.evidenceIds.join(", ")}</dd></>)}
        {plan.sourceDigest && (<><dt>Verdiğiniz metnin SHA-256 özeti</dt><dd><code>{plan.sourceDigest.slice(0, 16)}…</code> <span className="muted">(bir dosyanın değil, yapıştırdığınız metnin özeti)</span></dd></>)}
      </dl>
      {plan.untouched.length > 0 && <p className="small muted">Bu önizlemenin hiçbir zaman dokunmadığı dosyalar: {plan.untouched.join("; ")}.</p>}

      {plan.problems.length > 0 && (
        <ul className="small bad-text" aria-label="Sorunlar">{plan.problems.map((p) => <li key={p.code + p.text}>{p.text}</li>)}</ul>
      )}
      {plan.blockers.length > 0 && (
        <div className="small">
          <strong>Neden uygulanabilir bir tarif yok:</strong>
          <ul aria-label="Engeller">{plan.blockers.map((b) => <li key={b.code + b.text}>{b.text}</li>)}</ul>
        </div>
      )}
      {plan.existingServerNames.length > 0 && (
        <p className="small">Mevcut metinde tanımlı sunucu adları (değerler gösterilmez): {plan.existingServerNames.map((n, i) => <span key={n}>{i > 0 && ", "}<code>{n}</code></span>)}.</p>
      )}

      {plan.proposed !== null && (
        <>
          {plan.hypothetical && <p role="note" className="bad-text"><strong>AÇIKLAYICI — bu bir tarif değildir.</strong> Katalog durumu izin vermediği için aşağıdaki fark yalnızca “bildirim onaylansaydı dosya nasıl görünürdü” sorusunu açıklar.</p>}
          <div className="cfg-sbs">
            <figure>
              <figcaption className="small"><strong>Önce</strong> <span className="muted">(maskelenmiş, yeniden biçimlendirilmiş)</span></figcaption>
              <pre className="cfg-pre" tabIndex={0} aria-label="Önce">{plan.original ?? "(dosya yok — yeni dosya)"}</pre>
            </figure>
            <figure>
              <figcaption className="small"><strong>Sonra</strong> <span className="muted">(önizleme; uygulanmadı)</span></figcaption>
              <pre className="cfg-pre" tabIndex={0} aria-label="Sonra">{plan.proposed}</pre>
            </figure>
          </div>
          {plan.diff && (
            <figure>
              <figcaption className="small"><strong>Odaklı fark</strong></figcaption>
              <pre className="cfg-pre" tabIndex={0} aria-label="Odaklı fark">
                {plan.diff.map((l, i) => (
                  <span key={i} className={`cfg-${l.kind}`}>{l.kind === "add" ? "+ " : l.kind === "del" ? "- " : l.kind === "gap" ? "  " : "  "}{l.text}{"\n"}</span>
                ))}
              </pre>
            </figure>
          )}
        </>
      )}
      {plan.formattingNote && <p className="small muted">{plan.formattingNote}</p>}
      <details>
        <summary>Kaynak, uyumluluk ve güven kısıtları ({plan.constraints.length})</summary>
        <ul className="small">{plan.constraints.map((c) => <li key={c}>{c}</li>)}</ul>
      </details>
    </section>
  );
}

export function ConfigPreview({ entry, alias, projectName, platform, surface, declarations = REVIEWED_DECLARATIONS }: {
  entry: CatalogEntry;
  alias: string;
  projectName: string;
  platform: string;
  surface: string;
  declarations?: readonly ReviewedDeclaration[];
}) {
  const uid = useId();
  const [mode, setMode] = useState<Mode>("");
  const [text, setText] = useState("");
  const [plan, setPlan] = useState<PreviewPlan | null>(null);
  const [message, setMessage] = useState("");
  const modeRef = useRef<HTMLInputElement>(null);
  if (!isMcpEntry(entry)) return null;

  const request = (): PlanRequest => ({ alias, platform, surface, entry, source: mode === "text" ? { kind: "text", text } : { kind: "new" } });
  const stale = plan !== null && isPlanStale(plan, request(), declarations);

  const preview = () => {
    if (mode === "") {
      setMessage("Önce mevcut yapılandırmayı seçin: “Yeni dosya” ya da “Var olan dosyanın metnini yapıştırıyorum”. Hiçbir dosya okunmadı ve hiçbir önizleme üretilmedi.");
      setPlan(null);
      modeRef.current?.focus();
      return;
    }
    setMessage("");
    setPlan(planConfigPreview(request(), declarations));
  };

  return (
    <details className="cfg-preview">
      <summary>Yapılandırma önizlemesi — salt okunur, hiçbir dosyaya yazmaz</summary>
      <p className="small">
        Bu bölüm bir dosyayı <strong>okumaz ve yazmaz</strong>, bir şey kurmaz, çalıştırmaz, ağa bağlanmaz ve tercihinizi değiştirmez. Yalnızca, onaylı bir bildirim varsa, bu projenin
        MCP yapılandırma dosyasının nasıl değişeceğini sizin verdiğiniz metin üzerinden gösterir. Karar yine yalnızca yukarıdaki alanlardan, sizin açık eyleminizle kaydedilir.
      </p>
      <fieldset className="field">
        <legend>Mevcut yapılandırma ({surfaceLabel(surface)} · {platformLabel(platform)})</legend>
        <div>
          <label><input ref={modeRef} type="radio" name={`${uid}-mode`} value="new" checked={mode === "new"} onChange={() => { setMode("new"); setMessage(""); }} /> Yeni dosya (dosya henüz yok)</label>
        </div>
        <div>
          <label><input type="radio" name={`${uid}-mode`} value="text" checked={mode === "text"} onChange={() => { setMode("text"); setMessage(""); }} /> Var olan dosyanın metnini ben yapıştırıyorum</label>
        </div>
        {mode === "text" && (
          <div className="field">
            <label htmlFor={`${uid}-t`}>Dosya metni <span className="sr-only">({entry.displayName})</span></label>
            <textarea id={`${uid}-t`} rows={8} autoComplete="off" spellCheck={false} value={text} onChange={(e) => setText(e.target.value)} aria-describedby={`${uid}-hint`} />
            <div id={`${uid}-hint`} className="small muted">
              Uygulama hiçbir dosyayı kendisi okumaz. Anahtar, belirteç veya parola yapıştırmamaya çalışın; gizli görünen değerler yine de maskelenir ve metin hiçbir yere kaydedilmez, kopyalanmaz.
            </div>
          </div>
        )}
      </fieldset>
      <p className="actions">
        <button type="button" className="secondary" onClick={preview}>
          Değişiklik önizle<span className="sr-only"> ({entry.displayName}; hiçbir dosyaya yazmaz)</span>
        </button>
      </p>
      {message && <p role="status" className="bad-text">{message}</p>}
      {stale && <p role="status" className="bad-text">Girdi değişti (metin, seçim veya katalog kaydı); önceki önizleme eskidi ve gizlendi. Yeniden “Değişiklik önizle” deyin.</p>}
      {plan && !stale && <PlanView plan={plan} projectName={projectName} entryName={entry.displayName} />}
    </details>
  );
}
