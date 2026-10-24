import { useEffect, useRef, useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, SyntheticBanner, useLoad } from "../components/common";
import { answerProblem, changedSince, parseEvidenceFiles, toTaskInput, type EvalSnapshot, type FrontendScope } from "../evaluation";
import { platformLabel, surfaceLabel } from "../labels";
import type { AppError, Evaluation, Recommendation } from "../types";
import { TARGET_PLATFORMS as PLATFORMS, TARGET_SURFACES as SURFACES, type TargetRequest } from "../workspace";

const newTaskId = () => `gorev-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

type OpenInMatrix = (request: TargetRequest & { entryId?: string }) => void;

/**
 * One suggestion. It is information plus a way to LOOK at the tool in the Project Tool Workspace - nothing else. The only place a project decision
 * is stored or forgotten is that matrix; this card holds no decision control. It points at the target of the evaluation that produced it
 * (`snapshot`), never at whatever the form says now.
 */
function RecommendationCard({ rec, snapshot, projectName, onOpen }: { rec: Recommendation; snapshot: EvalSnapshot; projectName: string; onOpen: OpenInMatrix }) {
  return (
    <section className="panel" aria-labelledby={`r-${rec.entryId}`}>
      <h3 id={`r-${rec.entryId}`}>
        {rec.displayName}{" "}
        <span className={`badge ${rec.level === "suggest" ? "badge-passed" : "badge-unknown"}`}>{rec.level === "suggest" ? "Öneri" : "Yalnızca bilgi"}</span>
      </h3>
      <p>{rec.rationale}</p>
      <p className="small"><strong>Ne olur?</strong> {rec.impact}</p>
      <p className="small muted">Dayanak: {rec.signals.join("; ")}</p>
      {rec.notes.length > 0 && <ul className="small">{rec.notes.map((n) => <li key={n}>{n}</li>)}</ul>}
      {rec.skillCandidates.length > 0 && (
        <div>
          <p className="small"><strong>En fazla {rec.skillCandidates.length} aday yetenek</strong> (hiçbiri kurulmaz; kaynak, sürüm ve lisans ayrıca incelenmeli):</p>
          <ul className="small">
            {rec.skillCandidates.map((c) => (
              <li key={c.skillId}>
                <strong>{c.name}</strong> — {c.description} <span className="muted">(kaynak {c.sourceSha.slice(0, 8)}, lisans {c.licenseSpdx ?? "bilinmiyor"})</span>
                {c.licenseWarning && <span className="bad-text"> ⚠ lisans bilinmiyor</span>}
                {c.conflictsWith.length > 0 && <span className="bad-text"> ⚠ şunlarla çakışabilir: {c.conflictsWith.join(", ")}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}
      <p className="small muted">
        Bu öneri yalnızca bilgidir; bu ekranda hiçbir tercih kaydedilmez, hiçbir şey kurulmaz veya etkinleştirilmez. Kararı, gerçek uyumluluk bilgisiyle birlikte
        Proje Araç Matrisi’nde siz verirsiniz.
        {!rec.canEnable && ` Matriste “kullanmak istiyorum” seçilemez: ${rec.entryId === "strix" ? "kapsamlı yazılı yetki olmadan çalıştırılamaz" : "uyumluluk bilinmiyor"}.`}
      </p>
      <p className="actions">
        <button type="button" className="secondary" onClick={() => onOpen({ alias: snapshot.alias, platform: snapshot.platform, surface: snapshot.surface, entryId: rec.entryId })}>
          Bu öneriyi matriste incele{" "}
          <span className="sr-only">({rec.displayName}; {projectName} · {platformLabel(snapshot.platform)} · {surfaceLabel(snapshot.surface)})</span>
        </button>
      </p>
    </section>
  );
}

export function RecommendationsPage({ onOpenInMatrix }: { onOpenInMatrix: OpenInMatrix }) {
  const projects = useLoad(api.listProjects);
  const scenarios = useLoad(api.listScenarios);
  const [alias, setAlias] = useState("");
  const [surface, setSurface] = useState<string>(SURFACES[0]);
  const [platform, setPlatform] = useState<string>(PLATFORMS[0]);
  const [taskText, setTaskText] = useState("");
  const [files, setFiles] = useState("");
  const [late, setLate] = useState(false);
  const [frontendScope, setFrontendScope] = useState<FrontendScope>("none");
  const [taskId, setTaskId] = useState(newTaskId);
  // A result is stored WITH the snapshot it answers, so what it says and what it points at can never drift to the form's current values.
  const [result, setResult] = useState<{ snapshot: EvalSnapshot; evaluation: Evaluation } | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(false);
  const [dropped, setDropped] = useState("");
  const [projectNote, setProjectNote] = useState("");

  // Every input of an evaluation, as of this render.
  const current: EvalSnapshot = { alias, platform, surface, taskId, taskText, evidenceFiles: parseEvidenceFiles(files), phase: late ? "late" : "start", frontendScope };

  // ---- latest request wins ------------------------------------------------------------------------------------------------------
  // `generation` numbers the evaluations; `inFlight` is the one that may still finish. Every input handler, a new task, the loss of the project
  // and unmounting call `cancelPending` BEFORE the input changes, after which that evaluation can change nothing: not the result, not the error,
  // not `loading`. A new input must therefore call `edited()` too (the tests change each existing input on its own and would fail otherwise).
  const generation = useRef(0);
  const inFlight = useRef<{ taskId: string } | null>(null);
  useEffect(() => () => {
    generation.current += 1;
    inFlight.current = null;
  }, []);

  const cancelPending = () => {
    const f = inFlight.current;
    if (!f) return;
    generation.current += 1;
    inFlight.current = null;
    setLoading(false);
    setDropped("Girdileri değiştirdiğiniz için süren değerlendirmenin sonucu atıldı ve gösterilmeyecek. Yeniden değerlendirebilirsiniz.");
    // the abandoned evaluation may already have noted its suggestions as "shown" for this task although nobody saw them: do not reuse its task id
    if (f.taskId === taskId) setTaskId(newTaskId());
  };
  /** Called by every input before it changes. */
  const edited = () => {
    cancelPending();
    setError(null);
    setProjectNote("");
  };

  useEffect(() => {
    const list = projects.data;
    if (!list) return;
    if (alias === "") {
      if (list.length > 0) setAlias(list[0].alias);
      return;
    }
    if (!list.some((p) => p.alias === alias)) {
      // the project the page was working on is gone: nothing evaluated for it may stay on screen
      cancelPending();
      setResult(null);
      setError(null);
      setTaskId(newTaskId());
      setAlias(list[0]?.alias ?? "");
      setProjectNote(list.length > 0 ? `Seçili proje (“${alias}”) artık listede yok; sonuçlar temizlendi ve ilk proje seçildi.` : `Seçili proje (“${alias}”) artık listede yok ve başka proje kalmadı; sonuçlar temizlendi.`);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects.data, alias]);

  // A task belongs to ONE project, agent and platform. Changing any of them starts a new task, so one
  // project's "already suggested" state is never carried into another (RT-1B-01).
  const restartTask = () => {
    cancelPending();
    setTaskId(newTaskId());
    setResult(null);
    setError(null);
    setProjectNote("");
  };

  const pickScenario = (id: string) => {
    const s = scenarios.data?.find((x) => x.id === id);
    if (s) {
      edited();
      setTaskText(s.taskText);
      setFiles(s.evidenceFiles.join("\n"));
      setResult(null);
      setTaskId(newTaskId());
    }
  };

  const evaluate = async () => {
    if (inFlight.current || alias === "" || taskText.trim() === "") return; // a second click while one is running does nothing
    const snap = current; // everything the question is made of, frozen at the click
    const mine = ++generation.current;
    inFlight.current = { taskId: snap.taskId };
    setLoading(true);
    setError(null);
    setDropped("");
    setProjectNote("");
    let outcome: { evaluation: Evaluation } | { error: AppError };
    try {
      const evaluation = await api.evaluateTask(toTaskInput(snap));
      const problem = answerProblem(snap, evaluation);
      outcome = problem ? { error: { code: "EVALUATION_ANSWER_MISMATCH", message: `${problem} Sonuç gösterilmedi.` } } : { evaluation };
    } catch (e) {
      outcome = { error: toAppError(e) };
    }
    if (mine !== generation.current) return; // an older or cancelled evaluation: it owns nothing any more, not even `loading`
    inFlight.current = null;
    setLoading(false);
    if ("evaluation" in outcome) {
      setResult({ snapshot: snap, evaluation: outcome.evaluation });
    } else {
      setError(outcome.error);
      if (outcome.error.code === "PROJECT_NOT_FOUND") projects.reload(); // the list shown may be out of date
    }
  };

  const nameOf = (a: string) => projects.data?.find((p) => p.alias === a)?.displayName ?? a;
  const outdated = result ? changedSince(result.snapshot, current) : [];

  return (
    <>
      <SyntheticBanner>
        Bu bir <strong>öneri önizlemesidir</strong>. Gerçek bir Claude Code oturumuna bağlı değildir; yalnızca aşağıda seçtiğiniz örnek veya yazdığınız görev
        metnini değerlendirir. Hiçbir yapay zekâ modeli çağrılmaz, hiçbir şey kurulmaz, dosyalarınız taranmaz.
      </SyntheticBanner>
      <p className="muted">
        Kurallar sabittir ve bu bilgisayarda çalışır. Öneri için görevin <em>kendisi</em> ilgili olmalıdır; yalnızca depoda CSS dosyası bulunması yetmez. Aynı görev
        için aynı araç en fazla bir kez, bir görevde en fazla {2} öneri gösterilir. Bunu uygulayabilmek için her değerlendirme, hangi önerinin bu görevde gösterildiğini
        yalnızca bu bilgisayardaki uygulama kaydına yazar; tercih kaydedilmez. Öneriler yalnızca bilgidir: kararı Proje Araç Matrisi’nde siz verirsiniz.
      </p>
      {projects.error && <ErrorNotice error={projects.error} onRetry={projects.reload} />}
      <div className="panel">
        <div className="field">
          <label htmlFor="rec-scenario">Örnek görev (sentetik)</label>
          <select id="rec-scenario" defaultValue="" onChange={(e) => pickScenario(e.target.value)}>
            <option value="">Örnek seçin…</option>
            {scenarios.data?.map((s) => <option key={s.id} value={s.id}>{s.title}</option>)}
          </select>
        </div>
        <div className="field">
          <label htmlFor="rec-text">Görev metni</label>
          <textarea id="rec-text" rows={3} maxLength={2000} value={taskText} onChange={(e) => { edited(); setTaskText(e.target.value); }} />
        </div>
        <div className="field">
          <label htmlFor="rec-files">İlgili dosya adları (her satıra bir ad; yalnızca ad, uygulama dosyayı okumaz)</label>
          <textarea id="rec-files" rows={3} value={files} onChange={(e) => { edited(); setFiles(e.target.value); }} />
        </div>
        <div className="row">
          <div className="field">
            <label htmlFor="rec-project">Proje</label>
            <select id="rec-project" value={alias} onChange={(e) => { edited(); setAlias(e.target.value); restartTask(); }}>
              {projects.data?.map((p) => <option key={p.alias} value={p.alias}>{p.displayName}{p.kind === "synthetic" ? " (örnek)" : ""}</option>)}
            </select>
          </div>
          <div className="field">
            <label htmlFor="rec-surface">Ajan yüzeyi</label>
            <select id="rec-surface" value={surface} onChange={(e) => { edited(); setSurface(e.target.value); restartTask(); }}>{SURFACES.map((s) => <option key={s} value={s}>{surfaceLabel(s)}</option>)}</select>
          </div>
          <div className="field">
            <label htmlFor="rec-platform">İşletim sistemi</label>
            <select id="rec-platform" value={platform} onChange={(e) => { edited(); setPlatform(e.target.value); restartTask(); }}>{PLATFORMS.map((s) => <option key={s} value={s}>{platformLabel(s)}</option>)}</select>
          </div>
        </div>
        <p className="small">
          <button type="button" className="secondary" disabled={alias === ""} onClick={() => onOpenInMatrix({ alias, platform, surface })}>Bu hedefi Proje Araç Matrisi’nde aç</button>{" "}
          <span className="muted">Yalnızca ekranı değiştirir: görev değerlendirilmez, hiçbir şey kurulmaz, etkinleştirilmez veya kaydedilmez.</span>
        </p>
        <div className="field">
          <label htmlFor="rec-scope">Varsayımsal durum: Frontend Design eklentisi zaten…</label>
          <select id="rec-scope" value={frontendScope} onChange={(e) => { edited(); setFrontendScope(e.target.value as FrontendScope); }}>
            <option value="none">kurulu görünmüyor</option>
            <option value="global">genel olarak kurulu ve açık</option>
            <option value="project">bu projede kurulu ve açık</option>
          </select>
          <div className="muted small">Gerçek kurulum denetlenmediği için bu bilgiyi siz varsayım olarak verirsiniz; hiçbir yere kaydedilmez.</div>
        </div>
        <label className="choice">
          <input type="checkbox" checked={late} onChange={(e) => { edited(); setLate(e.target.checked); }} />
          Görev ortasında yeni bir alan fark edildi (geç öneri; görev başına en fazla bir)
        </label>
        <p>
          <button type="button" onClick={evaluate} disabled={alias === "" || taskText.trim() === "" || loading} aria-busy={loading}>Değerlendir</button>{" "}
          <button type="button" className="secondary" onClick={restartTask}>Yeni görev başlat</button>
          <span className="small muted"> Görev kimliği: {taskId}</span>
        </p>
      </div>
      <div role="status" className="small">
        {loading ? "Değerlendiriliyor…" : dropped || projectNote}
      </div>
      {error && <ErrorNotice error={error} />}
      <div aria-live="polite">
        {result && (
          <section aria-labelledby="h-results">
            <h2 id="h-results">Sonuç</h2>
            <p className="small">
              Değerlendirilen hedef: <strong>{nameOf(result.snapshot.alias)}</strong> · {platformLabel(result.snapshot.platform)} · {surfaceLabel(result.snapshot.surface)}
              {result.snapshot.phase === "late" ? " · geç öneri" : ""}
            </p>
            {outdated.length > 0 && (
              <p className="small bad-text" role="note">
                ⚠ Bu sonuç formdaki önceki girdilerle üretildi; şunları değiştirdiniz: {outdated.join(", ")}. Güncel girdiler için yeniden değerlendirin; aşağıdaki
                öneriler yine de yukarıdaki hedef için geçerlidir.
              </p>
            )}
            <p className="small muted">{result.evaluation.disclaimer}</p>
            {result.evaluation.recommendations.length === 0 && <p><strong>Bu görev için öneri yok.</strong> Gereksiz soru sormamak bilinçli bir davranıştır.</p>}
            {result.evaluation.recommendations.map((r) => (
              <RecommendationCard key={r.entryId} rec={r} snapshot={result.snapshot} projectName={nameOf(result.snapshot.alias)} onOpen={onOpenInMatrix} />
            ))}
            {result.evaluation.suppressed.length > 0 && (
              <details>
                <summary>Gösterilmeyenler ve nedenleri ({result.evaluation.suppressed.length})</summary>
                <ul className="small">{result.evaluation.suppressed.map((s) => <li key={`${s.entryId}-${s.reasonCode}`}><strong>{s.entryId}</strong>: {s.reason} <code>{s.reasonCode}</code></li>)}</ul>
              </details>
            )}
          </section>
        )}
      </div>
    </>
  );
}
