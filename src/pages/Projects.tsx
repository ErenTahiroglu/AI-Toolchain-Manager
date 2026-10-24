import { useEffect, useState, type FormEvent } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, SyntheticBanner, useLoad } from "../components/common";
import { ProjectToolWorkspace } from "../components/ProjectToolWorkspace";
import type { AppError, ProjectView } from "../types";
import { DEFAULT_TARGET, type MatrixHandoff, type TargetRequest } from "../workspace";

const targetOf = (h: TargetRequest): TargetRequest => ({ alias: h.alias, platform: h.platform, surface: h.surface });

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

/** A project record. Decisions are NOT edited here: the only place that stores or forgets a wish is the Project Tool Workspace above. */
function ProjectCard({ project, onChanged, onOpenInMatrix }: { project: ProjectView; onChanged: () => void; onOpenInMatrix: () => void }) {
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const synthetic = project.kind === "synthetic";
  const decisions = project.preferences.length;

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
      <p className="small">
        Kayıtlı karar: <strong>{decisions}</strong>. Kararlar yalnızca Proje Araç Matrisi’nde verilir ve değiştirilir; kayıtlı bir karar kurulu olduğu anlamına gelmez.{" "}
        <button type="button" className="secondary" onClick={onOpenInMatrix}>Araç matrisinde aç{" "}<span className="sr-only">({project.displayName})</span></button>
      </p>
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

/**
 * `handoff`: what another page asks to be opened - a target (project alias, operating system, agent surface) and optionally one suggested tool. It only
 * selects and points at what the matrix shows; it never decides, saves, evaluates or installs anything.
 */
export function ProjectsPage({ handoff = null }: { handoff?: MatrixHandoff | null }) {
  const projects = useLoad(api.listProjects);
  const catalog = useLoad(api.listCatalog);
  const [target, setTarget] = useState<TargetRequest>(handoff ? targetOf(handoff) : DEFAULT_TARGET);
  const [focusNonce, setFocusNonce] = useState(0);
  const reload = () => projects.reload();

  useEffect(() => {
    if (handoff) setTarget(targetOf(handoff));
  }, [handoff]);

  return (
    <>
      <p className="muted">
        Her kararın anahtarı: <strong>proje + araç + ajan yüzeyi + işletim sistemi</strong>. Kayıtlı karar sizin <em>isteğinizdir</em>; aracın gerçekten kurulu, doğrulanmış
        veya bir ajanda etkin olduğu anlamına gelmez. Kararlar aşağıdaki “Proje Araç Matrisi”nde verilir; “Gerçek durum” orada ayrı gösterilir.
      </p>
      <SyntheticBanner>
        "Örnek" etiketli projeler uydurmadır; bilgisayarınızdaki gerçek klasörlerle ilgisi yoktur. Gerçek proje kayıtlarınız ayrıca etiketlenir.
      </SyntheticBanner>
      <ProjectToolWorkspace projects={projects} catalog={catalog} target={target} onTarget={setTarget} focusNonce={focusNonce} focus={handoff} />
      <AddReference onAdded={reload} />
      {projects.error && <ErrorNotice error={projects.error} onRetry={projects.reload} />}
      {projects.loading && !projects.data && <p role="status">Yükleniyor…</p>}
      {projects.data?.map((p) => (
        <ProjectCard
          key={p.projectId}
          project={p}
          onChanged={reload}
          onOpenInMatrix={() => { setTarget((t) => ({ ...t, alias: p.alias })); setFocusNonce((n) => n + 1); }}
        />
      ))}
    </>
  );
}
