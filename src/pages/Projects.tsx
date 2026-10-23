import { useState, type FormEvent } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, SyntheticBanner, useLoad } from "../components/common";
import { ProjectToolWorkspace } from "../components/ProjectToolWorkspace";
import { compatLabel, decisionLabel, formatTime, installLabel, platformLabel, scopeLabel, surfaceLabel, validationLabel } from "../labels";
import type { AppError, CatalogEntry, PreferenceView, ProjectView } from "../types";

const SURFACES = ["claude-code-terminal", "claude-desktop-code", "antigravity-ide", "local-cli"];
const PLATFORMS = ["windows-x64", "macos-arm64"];
const DECISIONS: { value: string; label: string }[] = [
  { value: "accepted", label: "Bu proje için etkinleştirmek istiyorum" },
  { value: "postponed", label: "Şimdi değil (ertele)" },
  { value: "rejected", label: "Reddet (tekrar sorma)" },
  { value: "disabled", label: "Bu proje için kapalı tut" },
];
const ALIAS_HINT = "2-42 karakter: küçük harf, rakam ve tire";

function AddReference({ onAdded }: { onAdded: () => void }) {
  const [alias, setAlias] = useState("");
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<AppError | null>(null);

  const submit = async (ev: FormEvent) => {
    ev.preventDefault();
    setError(null);
    try {
      await api.addProjectReference(alias, name, note);
      setAlias(""); setName(""); setNote("");
      onAdded();
    } catch (e) {
      setError(toAppError(e));
    }
  };

  return (
    <form className="panel" onSubmit={submit} aria-labelledby="h-addproj">
      <h2 id="h-addproj">Gerçek bir proje kaydı ekle</h2>
      <p className="muted small">
        Burada yalnızca bir <strong>kayıt</strong> oluşur. Uygulama klasörünüzü açmaz, taramaz, dosyalarınıza veya ajan ayarlarınıza dokunmaz.
        Notunuz yalnızca bu bilgisayarda, sizin için tutulur; hiçbir yere gönderilmez ve günlüklere yazılmaz.
      </p>
      <div className="field">
        <label htmlFor="proj-alias">Takma ad ({ALIAS_HINT})</label>
        <input id="proj-alias" type="text" autoComplete="off" spellCheck={false} value={alias} onChange={(e) => setAlias(e.target.value)} />
      </div>
      <div className="field">
        <label htmlFor="proj-name">Görünen ad</label>
        <input id="proj-name" type="text" maxLength={80} value={name} onChange={(e) => setName(e.target.value)} />
      </div>
      <div className="field">
        <label htmlFor="proj-note">Özel not (isteğe bağlı; örneğin klasörün yeri — yalnızca hatırlatma)</label>
        <input id="proj-note" type="text" maxLength={200} value={note} onChange={(e) => setNote(e.target.value)} />
      </div>
      <p><button type="submit" disabled={alias.trim() === "" || name.trim() === ""}>Proje kaydını ekle</button></p>
      {error && <ErrorNotice error={error} />}
    </form>
  );
}

function PreferenceRow({ project, pref, onChanged }: { project: ProjectView; pref: PreferenceView; onChanged: () => void }) {
  const [error, setError] = useState<AppError | null>(null);
  const reset = async () => {
    setError(null);
    try {
      await api.resetProjectPreference({ alias: project.alias, entryId: pref.entryId, agentSurface: pref.agentSurface, platformTarget: pref.platformTarget });
      onChanged();
    } catch (e) {
      setError(toAppError(e));
    }
  };
  return (
    <tr>
      <th scope="row">{pref.entryName}{pref.entryRemoved && <span className="tag">katalogdan kaldırıldı</span>}</th>
      <td>{surfaceLabel(pref.agentSurface)}<br /><span className="small muted">{platformLabel(pref.platformTarget)}</span></td>
      <td>
        <strong>{decisionLabel(pref.decision)}</strong>
        <div className="small muted">{formatTime(pref.decidedAt)} · sürüm {pref.catalogRevision}</div>
        {pref.remindAt && <div className="small muted">Hatırlatma: {formatTime(pref.remindAt)}</div>}
        {pref.materialChange && <div className="small bad-text">Aracın yetenekleri/uyumluluğu bu karardan sonra önemli ölçüde değişti.</div>}
        {!pref.materialChange && pref.revisionChanged && <div className="small muted">Yeni sürüm var; bu tek başına yeniden sormaz.</div>}
      </td>
      <td className="small">
        {installLabel(pref.actualInstallState)}<br />
        {validationLabel(pref.actualVerificationState)}<br />
        {scopeLabel(pref.observedScope)}<br />
        <span className="muted">Uyumluluk: {compatLabel(pref.compatibility)}</span>
      </td>
      <td>
        <button type="button" className="secondary" onClick={reset}>Kararı sıfırla</button>
        {error && <ErrorNotice error={error} />}
      </td>
    </tr>
  );
}

function PreferenceForm({ project, entries, onSaved }: { project: ProjectView; entries: CatalogEntry[]; onSaved: () => void }) {
  const [entryId, setEntryId] = useState("");
  const [surface, setSurface] = useState(SURFACES[0]);
  const [platform, setPlatform] = useState(PLATFORMS[0]);
  const [decision, setDecision] = useState("accepted");
  const [days, setDays] = useState(7);
  const [error, setError] = useState<AppError | null>(null);

  const submit = async (ev: FormEvent) => {
    ev.preventDefault();
    setError(null);
    try {
      await api.setProjectPreference({ alias: project.alias, entryId, agentSurface: surface, platformTarget: platform }, decision, decision === "postponed" ? days : undefined);
      onSaved();
    } catch (e) {
      setError(toAppError(e));
    }
  };

  return (
    <form onSubmit={submit} aria-label={`${project.displayName} için karar ekle`} className="inline-form">
      <h3>Karar ekle</h3>
      <div className="field">
        <label htmlFor={`pe-${project.alias}`}>Kayıt</label>
        <select id={`pe-${project.alias}`} value={entryId} onChange={(e) => setEntryId(e.target.value)}>
          <option value="">Seçin</option>
          {entries.map((e) => <option key={e.entryId} value={e.entryId}>{e.displayName}</option>)}
        </select>
      </div>
      <div className="field">
        <label htmlFor={`ps-${project.alias}`}>Ajan yüzeyi</label>
        <select id={`ps-${project.alias}`} value={surface} onChange={(e) => setSurface(e.target.value)}>{SURFACES.map((s) => <option key={s} value={s}>{surfaceLabel(s)}</option>)}</select>
      </div>
      <div className="field">
        <label htmlFor={`pp-${project.alias}`}>İşletim sistemi</label>
        <select id={`pp-${project.alias}`} value={platform} onChange={(e) => setPlatform(e.target.value)}>{PLATFORMS.map((s) => <option key={s} value={s}>{platformLabel(s)}</option>)}</select>
      </div>
      <div className="field">
        <label htmlFor={`pd-${project.alias}`}>Karar</label>
        <select id={`pd-${project.alias}`} value={decision} onChange={(e) => setDecision(e.target.value)}>{DECISIONS.map((d) => <option key={d.value} value={d.value}>{d.label}</option>)}</select>
      </div>
      {decision === "postponed" && (
        <div className="field">
          <label htmlFor={`pr-${project.alias}`}>Kaç gün sonra hatırlat</label>
          <input id={`pr-${project.alias}`} type="number" min={1} max={365} value={days} onChange={(e) => setDays(Number(e.target.value))} />
        </div>
      )}
      <p className="muted small">Bu yalnızca <strong>isteğinizi</strong> kaydeder. Hiçbir şey kurulmaz, etkinleştirilmez veya doğrulanmaz.</p>
      <p><button type="submit" disabled={entryId === ""}>Kararı kaydet</button></p>
      {error && <ErrorNotice error={error} />}
    </form>
  );
}

function ProjectCard({ project, entries, onChanged }: { project: ProjectView; entries: CatalogEntry[]; onChanged: () => void }) {
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const synthetic = project.kind === "synthetic";

  const remove = async () => {
    setError(null);
    try {
      await api.removeProjectReference(project.alias);
      onChanged();
    } catch (e) {
      setError(toAppError(e));
    }
  };

  return (
    <section className="panel" aria-labelledby={`p-${project.alias}`}>
      <h2 id={`p-${project.alias}`}>
        {project.displayName} <span className="tag">{synthetic ? "örnek" : "gerçek proje kaydı"}</span>
      </h2>
      <p className="small muted">Takma ad: <code>{project.alias}</code></p>
      {synthetic ? (
        <p className="muted">Uydurma örnek proje. Gerçek bir klasörünüzle ilgisi yoktur.</p>
      ) : (
        <p className="small">Özel not: {project.localNote || <span className="muted">(yok)</span>} <span className="muted">— uygulama bu klasöre hiç dokunmaz.</span></p>
      )}
      {project.preferences.length === 0 ? (
        <p className="muted">Henüz kayıtlı karar yok.</p>
      ) : (
        <div className="table-wrap">
          <table>
            <caption className="sr-only">{project.displayName} için kayıtlı kararlar</caption>
            <thead>
              <tr><th scope="col">Araç</th><th scope="col">Ajan / sistem</th><th scope="col">İsteğiniz</th><th scope="col">Gerçek durum</th><th scope="col">İşlem</th></tr>
            </thead>
            <tbody>{project.preferences.map((p) => <PreferenceRow key={`${p.entryId}-${p.agentSurface}-${p.platformTarget}`} project={project} pref={p} onChanged={onChanged} />)}</tbody>
          </table>
        </div>
      )}
      <PreferenceForm project={project} entries={entries} onSaved={onChanged} />
      {!synthetic && (
        <p>
          {!confirming ? (
            <button type="button" className="secondary" onClick={() => setConfirming(true)}>Proje kaydını kaldır</button>
          ) : (
            <span role="group" aria-label="Kaldırmayı onayla">
              <span className="small">Yalnızca bu uygulamadaki kayıt kaldırılır; klasörünüze hiçbir şey olmaz. </span>
              <button type="button" onClick={remove}>Evet, kaldır</button>{" "}
              <button type="button" className="secondary" onClick={() => setConfirming(false)}>Vazgeç</button>
            </span>
          )}
        </p>
      )}
      {error && <ErrorNotice error={error} />}
    </section>
  );
}

export function ProjectsPage() {
  const projects = useLoad(api.listProjects);
  const catalog = useLoad(api.listCatalog);
  const entries = (catalog.data ?? []).filter((e) => e.removedAt === null);
  const reload = () => projects.reload();

  return (
    <>
      <p className="muted">
        Her kararın anahtarı: <strong>proje + araç + ajan yüzeyi + işletim sistemi</strong>. Kayıtlı karar sizin <em>isteğinizdir</em>; aracın gerçekten kurulu veya
        doğrulanmış olması anlamına gelmez ("Gerçek durum" sütunu ayrıdır ve bu sürümde hep "kurulu değil / doğrulanmadı / bilinmiyor"dur).
      </p>
      <SyntheticBanner>
        "Örnek" etiketli projeler uydurmadır; bilgisayarınızdaki gerçek klasörlerle ilgisi yoktur. Gerçek proje kayıtlarınız ayrıca etiketlenir.
      </SyntheticBanner>
      <ProjectToolWorkspace projects={projects} catalog={catalog} />
      <AddReference onAdded={reload} />
      {projects.error && <ErrorNotice error={projects.error} onRetry={projects.reload} />}
      {projects.loading && !projects.data && <p role="status">Yükleniyor…</p>}
      {projects.data?.map((p) => <ProjectCard key={p.projectId} project={p} entries={entries} onChanged={reload} />)}
    </>
  );
}
