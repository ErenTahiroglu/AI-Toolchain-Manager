import { useEffect, useState, type FormEvent } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, UnavailableButton, useLoad } from "../components/common";
import {
  KIND_HELP, compatLabel, installLabel, kindLabel, licenseLabel, metadataLabel, platformLabel, policyLabel, reviewLabel, surfaceLabel,
  validationLabel,
} from "../labels";
import type { AppError, CatalogEntry, EntryKind, ExecutionBlock, UrlPreview } from "../types";

const KINDS: EntryKind[] = ["executable_tool", "agent_plugin", "skill_collection", "prompt_source"];

function AddForm({ onAdded }: { onAdded: (e: CatalogEntry) => void }) {
  const [url, setUrl] = useState("");
  const [kind, setKind] = useState<EntryKind | "">("");
  const [purpose, setPurpose] = useState("");
  const [preview, setPreview] = useState<UrlPreview | null>(null);
  const [previewedUrl, setPreviewedUrl] = useState("");
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);

  const checkUrl = async () => {
    setError(null);
    try {
      setPreview(await api.previewGithubUrl(url));
      setPreviewedUrl(url);
    } catch (e) {
      setError(toAppError(e));
    }
  };

  const canAdd = !!preview?.ok && previewedUrl === url && kind !== "" && !busy;

  const submit = async (ev: FormEvent) => {
    ev.preventDefault();
    if (!canAdd) return;
    setBusy(true);
    setError(null);
    try {
      const entry = await api.addCatalogEntry(url, kind, purpose);
      setUrl(""); setKind(""); setPurpose(""); setPreview(null); setPreviewedUrl("");
      onAdded(entry);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="panel" onSubmit={submit} aria-labelledby="h-add">
      <h2 id="h-add">GitHub deposu ekle</h2>
      <p className="muted small">
        Eklemek yalnızca <strong>kayıt oluşturur</strong>. Hiçbir şey indirilmez, kurulmaz veya çalıştırılmaz; GitHub'a bağlanılmaz.
        Depodaki betikler ve kurulum komutları asla çalıştırılmaz veya tahmin edilmez.
      </p>
      <div className="field">
        <label htmlFor="repo-url">GitHub bağlantısı</label>
        <input id="repo-url" type="text" inputMode="url" autoComplete="off" spellCheck={false} placeholder="https://github.com/sahip/depo" value={url} onChange={(e) => setUrl(e.target.value)} />
      </div>
      <div className="field">
        <label htmlFor="repo-kind">Bu bir… (tür)</label>
        <select id="repo-kind" value={kind} onChange={(e) => setKind(e.target.value as EntryKind | "")}>
          <option value="">Tür seçin</option>
          {KINDS.map((k) => <option key={k} value={k}>{kindLabel(k)}</option>)}
        </select>
        {kind !== "" && <div className="muted small">{KIND_HELP[kind]}</div>}
      </div>
      <div className="field">
        <label htmlFor="repo-purpose">Ne işe yarıyor? (isteğe bağlı, yalnızca sizin notunuz)</label>
        <input id="repo-purpose" type="text" maxLength={300} value={purpose} onChange={(e) => setPurpose(e.target.value)} />
      </div>
      <p>
        <button type="button" onClick={checkUrl} disabled={url.trim() === ""}>Bağlantıyı denetle</button>{" "}
        <button type="submit" disabled={!canAdd}>Aday olarak ekle</button>
      </p>
      {error && <ErrorNotice error={error} />}
      {preview && (
        <div aria-live="polite" className="preview">
          {preview.error && <p role="alert" className="bad-text">✕ {preview.error.message} <code>{preview.error.code}</code></p>}
          {preview.parsed && (
            <p>
              Sahip: <strong>{preview.parsed.owner}</strong> · Depo: <strong>{preview.parsed.repo}</strong> · Adres: {preview.parsed.canonicalUrl}
            </p>
          )}
          {preview.duplicateOf && !preview.duplicateOf.removed && (
            <p role="alert" className="bad-text">Bu depo zaten katalogda: {preview.duplicateOf.displayName}. Yinelenen kayıt eklenmez.</p>
          )}
          {preview.duplicateOf?.removed && <p>Bu depo daha önce kaldırılmış; eklerseniz eski kayıt geri gelir (kapalı olarak).</p>}
          {preview.ok && <p>✓ Bağlantı geçerli. Tür seçip ekleyebilirsiniz.</p>}
          <ul className="small muted">{preview.notes.map((n) => <li key={n}>{n}</li>)}</ul>
        </div>
      )}
    </form>
  );
}

function ExecutionNote({ entryId }: { entryId: string }) {
  const [block, setBlock] = useState<ExecutionBlock | null>(null);
  useEffect(() => {
    api.explainExecutionBlock(entryId).then(setBlock).catch(() => setBlock(null));
  }, [entryId]);
  return <UnavailableButton label="Kur / çalıştır" reason={block?.message ?? "Faz 1B'de araç kurulumu ve çalıştırma yoktur."} />;
}

function EntryCard({ entry, onChanged }: { entry: CatalogEntry; onChanged: () => void }) {
  const [error, setError] = useState<AppError | null>(null);
  const [confirming, setConfirming] = useState(false);
  const removed = entry.removedAt !== null;

  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      setConfirming(false);
      onChanged();
    } catch (e) {
      setError(toAppError(e));
    }
  };

  const evidence = entry.record.targetEvidence ?? [];
  return (
    <section className={`panel${removed ? " removed" : ""}`} aria-labelledby={`e-${entry.entryId}`}>
      <h2 id={`e-${entry.entryId}`}>{entry.displayName}</h2>
      <p className="small">
        <span className="tag">{kindLabel(entry.kind)}</span>{" "}
        <span className="tag">{reviewLabel(entry.reviewState)}</span>{" "}
        <span className="tag">{installLabel(entry.installState)}</span>{" "}
        <span className="tag">{validationLabel(entry.verificationState)}</span>{" "}
        <span className="tag">{entry.origin === "user" ? "Sizin eklediğiniz aday" : "Hazır kayıt"}</span>
        {removed && <> <span className="tag">Katalogdan kaldırıldı</span></>}
      </p>
      <p>{entry.purpose}</p>
      <dl className="facts small">
        <dt>Kaynak</dt>
        <dd>{entry.repoUrl}{entry.sourcePath ? ` (klasör: ${entry.sourcePath})` : ""}</dd>
        {entry.pluginIdentity && (<><dt>Eklenti kimliği</dt><dd>{entry.pluginIdentity}</dd></>)}
        <dt>Bilgi durumu</dt><dd>{metadataLabel(entry.metadataState)}</dd>
        <dt>Lisans</dt><dd>{licenseLabel(entry.licenseState)}{entry.licenseSpdx ? `: ${entry.licenseSpdx}` : ""}</dd>
        <dt>Sürüm (gözlenen)</dt><dd>{entry.revision}</dd>
        <dt>Çalıştırma ilkesi</dt><dd>{policyLabel(entry.executionPolicy)}</dd>
        {entry.kind === "skill_collection" && (<><dt>Yetenek dizini</dt><dd>{entry.skillCount === 0 ? "Boş: henüz içe aktarılmadı (hiçbir yetenek kurulmaz)" : `${entry.skillCount} yetenek (yalnızca kimlik ve kaynak bilgisi)`}</dd></>)}
      </dl>
      <details>
        <summary>Sınırlamalar ve bilinmeyenler ({entry.limitations.length})</summary>
        <ul className="small">{entry.limitations.map((l) => <li key={l}>{l}</li>)}</ul>
      </details>
      {evidence.length > 0 && (
        <details>
          <summary>Platform ve ajan uyumluluğu ({evidence.length})</summary>
          <div className="table-wrap">
            <table>
              <thead><tr><th scope="col">Platform</th><th scope="col">Yüzey</th><th scope="col">Durum</th><th scope="col">Sınırlama</th></tr></thead>
              <tbody>
                {evidence.map((e) => (
                  <tr key={`${e.platformTarget}-${e.agentSurface}`}>
                    <td>{platformLabel(e.platformTarget)}</td><td>{surfaceLabel(e.agentSurface)}</td><td>{compatLabel(e.compatibilityState)}</td><td className="small">{e.limitation}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
      {error && <ErrorNotice error={error} />}
      {!removed ? (
        <div className="actions">
          <label className="choice">
            <input type="checkbox" checked={entry.enabled} onChange={(e) => run(() => api.setCatalogEntryEnabled(entry.entryId, e.target.checked))} />
            Katalogda etkin (öneri ve proje seçimlerinde kullanılsın)
          </label>
          <ExecutionNote entryId={entry.entryId} />
          {!confirming ? (
            <button type="button" className="secondary" onClick={() => setConfirming(true)}>Katalogdan kaldır</button>
          ) : (
            <span role="group" aria-label="Kaldırmayı onayla">
              <span className="small"> Yalnızca katalog kaydı kaldırılır; hiçbir şey kaldırılmaz (kurulu bir şey yok). </span>
              <button type="button" onClick={() => run(() => api.removeCatalogEntry(entry.entryId))}>Evet, kaydı kaldır</button>{" "}
              <button type="button" className="secondary" onClick={() => setConfirming(false)}>Vazgeç</button>
            </span>
          )}
        </div>
      ) : (
        <p><button type="button" onClick={() => run(() => api.restoreCatalogEntry(entry.entryId))}>Geri ekle (kapalı olarak)</button></p>
      )}
    </section>
  );
}

export function CatalogPage() {
  const { data, error, loading, reload } = useLoad(api.listCatalog);
  const [filter, setFilter] = useState<"all" | EntryKind>("all");
  const [notice, setNotice] = useState("");

  const live = (data ?? []).filter((e) => e.removedAt === null && (filter === "all" || e.kind === filter));
  const removed = (data ?? []).filter((e) => e.removedAt !== null);

  return (
    <>
      <p className="muted">
        Burası araç, eklenti, yetenek koleksiyonu ve istem kaynaklarının <strong>katalogudur</strong>. Hiçbiri kurulu değil ve hiçbirinin çalıştığı
        doğrulanmadı. Bu uygulama bu sürümde hiçbir şeyi kurmaz, çalıştırmaz veya ajan ayarlarına dokunmaz.
      </p>
      <AddForm onAdded={(e) => { setNotice(`${e.displayName} aday olarak eklendi. Hiçbir şey indirilmedi veya çalıştırılmadı.`); reload(); }} />
      {error && <ErrorNotice error={error} onRetry={reload} />}
      {loading && !data && <p role="status">Yükleniyor…</p>}
      {notice && <p role="status" className="ok-text">✓ {notice}</p>}
      <div className="field">
        <label htmlFor="kind-filter">Göster</label>
        <select id="kind-filter" value={filter} onChange={(e) => setFilter(e.target.value as "all" | EntryKind)}>
          <option value="all">Tümü ({(data ?? []).filter((e) => e.removedAt === null).length})</option>
          {KINDS.map((k) => <option key={k} value={k}>{kindLabel(k)}</option>)}
        </select>
      </div>
      {live.map((e) => <EntryCard key={e.entryId} entry={e} onChanged={reload} />)}
      {data && live.length === 0 && <p className="muted">Bu türde kayıt yok.</p>}
      {removed.length > 0 && (
        <section aria-labelledby="h-removed">
          <h2 id="h-removed">Katalogdan kaldırılanlar ({removed.length})</h2>
          {removed.map((e) => <EntryCard key={e.entryId} entry={e} onChanged={reload} />)}
        </section>
      )}
    </>
  );
}
