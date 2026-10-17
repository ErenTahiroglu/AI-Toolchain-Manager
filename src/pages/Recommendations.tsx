import { useEffect, useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, SyntheticBanner, useLoad } from "../components/common";
import { platformLabel, surfaceLabel } from "../labels";
import type { AppError, Evaluation, Recommendation } from "../types";

const SURFACES = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"];
const PLATFORMS = ["windows-x64", "macos-arm64"];
const FRONTEND = "anthropic-frontend-design";

const newTaskId = () => `gorev-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

function RecommendationCard({ rec, alias, surface, platform, onDecided }: { rec: Recommendation; alias: string; surface: string; platform: string; onDecided: (msg: string) => void }) {
  const [error, setError] = useState<AppError | null>(null);
  const [done, setDone] = useState("");
  const key = { alias, entryId: rec.entryId, agentSurface: surface, platformTarget: platform };

  const decide = async (decision: string, label: string, days?: number) => {
    setError(null);
    try {
      await api.setProjectPreference(key, decision, days);
      setDone(label);
      onDecided(`${rec.displayName}: ${label}`);
    } catch (e) {
      setError(toAppError(e));
    }
  };

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
      {done ? (
        <p role="status" className="ok-text">✓ Kaydedildi: {done}. Hiçbir şey kurulmadı.</p>
      ) : (
        <p className="actions">
          {rec.canEnable ? (
            <button type="button" onClick={() => decide("accepted", "Bu proje için etkinleştirmek istiyorum")}>Bu proje için etkinleştir?</button>
          ) : (
            <span className="small muted">Etkinleştirilemez: {rec.entryId === "strix" ? "kapsamlı yazılı yetki olmadan çalıştırılamaz" : "uyumluluk bilinmiyor"}. </span>
          )}
          <button type="button" className="secondary" onClick={() => decide("postponed", "Ertelendi (7 gün)", 7)}>Şimdi değil</button>
          <button type="button" className="secondary" onClick={() => decide("rejected", "Reddedildi, tekrar sorulmayacak")}>Reddet / bir daha sorma</button>
        </p>
      )}
      {error && <ErrorNotice error={error} />}
    </section>
  );
}

export function RecommendationsPage() {
  const projects = useLoad(api.listProjects);
  const scenarios = useLoad(api.listScenarios);
  const [alias, setAlias] = useState("");
  const [surface, setSurface] = useState(SURFACES[0]);
  const [platform, setPlatform] = useState(PLATFORMS[0]);
  const [taskText, setTaskText] = useState("");
  const [files, setFiles] = useState("");
  const [late, setLate] = useState(false);
  const [frontendScope, setFrontendScope] = useState<"none" | "global" | "project">("none");
  const [taskId, setTaskId] = useState(newTaskId);
  const [result, setResult] = useState<Evaluation | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [notice, setNotice] = useState("");

  useEffect(() => {
    if (!alias && projects.data && projects.data.length > 0) setAlias(projects.data[0].alias);
  }, [projects.data, alias]);

  // A task belongs to ONE project, agent and platform. Changing any of them starts a new task, so one
  // project's "already suggested" state is never carried into another (RT-1B-01).
  const restartTask = () => {
    setTaskId(newTaskId());
    setResult(null);
    setNotice("");
    setError(null);
  };

  const pickScenario = (id: string) => {
    const s = scenarios.data?.find((x) => x.id === id);
    if (s) {
      setTaskText(s.taskText);
      setFiles(s.evidenceFiles.join("\n"));
      setResult(null);
      setTaskId(newTaskId());
    }
  };

  const evaluate = async () => {
    setError(null);
    setNotice("");
    try {
      setResult(
        await api.evaluateTask({
          projectAlias: alias,
          taskId,
          taskText,
          evidenceFiles: files.split("\n").map((f) => f.trim()).filter(Boolean),
          agentSurface: surface,
          platformTarget: platform,
          phase: late ? "late" : "start",
          availability: frontendScope === "none" ? [] : [{ entryId: FRONTEND, scope: frontendScope, enabled: true }],
        }),
      );
    } catch (e) {
      setError(toAppError(e));
    }
  };

  return (
    <>
      <SyntheticBanner>
        Bu bir <strong>öneri önizlemesidir</strong>. Gerçek bir Claude Code oturumuna bağlı değildir; yalnızca aşağıda seçtiğiniz örnek veya yazdığınız görev
        metnini değerlendirir. Hiçbir yapay zekâ modeli çağrılmaz, hiçbir şey kurulmaz, dosyalarınız taranmaz.
      </SyntheticBanner>
      <p className="muted">
        Kurallar sabittir ve bu bilgisayarda çalışır. Öneri için görevin <em>kendisi</em> ilgili olmalıdır; yalnızca depoda CSS dosyası bulunması yetmez. Aynı görev
        için aynı araç en fazla bir kez, bir görevde en fazla {2} öneri gösterilir.
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
          <textarea id="rec-text" rows={3} maxLength={2000} value={taskText} onChange={(e) => setTaskText(e.target.value)} />
        </div>
        <div className="field">
          <label htmlFor="rec-files">İlgili dosya adları (her satıra bir ad; yalnızca ad, uygulama dosyayı okumaz)</label>
          <textarea id="rec-files" rows={3} value={files} onChange={(e) => setFiles(e.target.value)} />
        </div>
        <div className="row">
          <div className="field">
            <label htmlFor="rec-project">Proje</label>
            <select id="rec-project" value={alias} onChange={(e) => { setAlias(e.target.value); restartTask(); }}>
              {projects.data?.map((p) => <option key={p.alias} value={p.alias}>{p.displayName}{p.kind === "synthetic" ? " (örnek)" : ""}</option>)}
            </select>
          </div>
          <div className="field">
            <label htmlFor="rec-surface">Ajan yüzeyi</label>
            <select id="rec-surface" value={surface} onChange={(e) => { setSurface(e.target.value); restartTask(); }}>{SURFACES.map((s) => <option key={s} value={s}>{surfaceLabel(s)}</option>)}</select>
          </div>
          <div className="field">
            <label htmlFor="rec-platform">İşletim sistemi</label>
            <select id="rec-platform" value={platform} onChange={(e) => { setPlatform(e.target.value); restartTask(); }}>{PLATFORMS.map((s) => <option key={s} value={s}>{platformLabel(s)}</option>)}</select>
          </div>
        </div>
        <div className="field">
          <label htmlFor="rec-scope">Varsayımsal durum: Frontend Design eklentisi zaten…</label>
          <select id="rec-scope" value={frontendScope} onChange={(e) => setFrontendScope(e.target.value as "none" | "global" | "project")}>
            <option value="none">kurulu görünmüyor</option>
            <option value="global">genel olarak kurulu ve açık</option>
            <option value="project">bu projede kurulu ve açık</option>
          </select>
          <div className="muted small">Gerçek kurulum denetlenmediği için bu bilgiyi siz varsayım olarak verirsiniz; hiçbir yere kaydedilmez.</div>
        </div>
        <label className="choice">
          <input type="checkbox" checked={late} onChange={(e) => setLate(e.target.checked)} />
          Görev ortasında yeni bir alan fark edildi (geç öneri; görev başına en fazla bir)
        </label>
        <p>
          <button type="button" onClick={evaluate} disabled={alias === "" || taskText.trim() === ""}>Değerlendir</button>{" "}
          <button type="button" className="secondary" onClick={restartTask}>Yeni görev başlat</button>
          <span className="small muted"> Görev kimliği: {taskId}</span>
        </p>
      </div>
      {error && <ErrorNotice error={error} />}
      {notice && <p role="status" className="ok-text">✓ {notice}</p>}
      <div aria-live="polite">
        {result && (
          <section aria-labelledby="h-results">
            <h2 id="h-results">Sonuç</h2>
            <p className="small muted">{result.disclaimer}</p>
            {result.recommendations.length === 0 && <p><strong>Bu görev için öneri yok.</strong> Gereksiz soru sormamak bilinçli bir davranıştır.</p>}
            {result.recommendations.map((r) => (
              <RecommendationCard key={r.entryId} rec={r} alias={alias} surface={surface} platform={platform} onDecided={(m) => { setNotice(m); projects.reload(); }} />
            ))}
            {result.suppressed.length > 0 && (
              <details>
                <summary>Gösterilmeyenler ve nedenleri ({result.suppressed.length})</summary>
                <ul className="small">{result.suppressed.map((s) => <li key={`${s.entryId}-${s.reasonCode}`}><strong>{s.entryId}</strong>: {s.reason} <code>{s.reasonCode}</code></li>)}</ul>
              </details>
            )}
          </section>
        )}
      </div>
    </>
  );
}
