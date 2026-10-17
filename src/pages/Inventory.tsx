import { useMemo, useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, useLoad } from "../components/common";
import {
  configurationLabel, confidenceLabel, formatTime, methodLabel, obsScopeLabel, observedLabel, platformLabel, presenceLabel, subjectLabel, surfaceLabel,
} from "../labels";
import type { AppError, ScanRef, StoredObservation } from "../types";

const permissionText = (p: { authorizedPath: boolean; authorizedUserConfig: boolean; authorizedProject: boolean }) => {
  const parts = [
    p.authorizedPath ? "program adları (PATH)" : null,
    p.authorizedUserConfig ? "Claude Code kullanıcı dosyaları" : null,
    p.authorizedProject ? "bir proje klasörü" : null,
  ].filter(Boolean);
  return parts.length ? parts.join(" + ") : "hiçbir bölüm";
};

function ScanProvenance({ scan }: { scan: ScanRef }) {
  return (
    <div className="small muted">
      Tarama {scan.scanId.slice(0, 8)} · {formatTime(scan.finishedAt)} · izinler: {permissionText(scan)}
    </div>
  );
}

function PresenceBadge({ presence }: { presence: string }) {
  // Deliberately neutral: finding a file is evidence of presence, never a green "all good".
  return <span className={`badge ${presence === "unknown" || presence === "unsupported" ? "badge-skipped" : "badge-neutral"}`}>{presenceLabel(presence)}</span>;
}

function ObservationRow({ o }: { o: StoredObservation }) {
  return (
    <tr>
      <th scope="row">
        {obsScopeLabel(o.scope)}
        {o.projectAlias && <div className="small muted">proje: {o.projectAlias}</div>}
        <div className="small muted">{methodLabel(o.method)}</div>
      </th>
      <td><PresenceBadge presence={o.presence} /></td>
      <td className="small">{configurationLabel(o.configuration)}</td>
      <td className="small">{o.version ? <>{o.version}<div className="muted">({o.versionSource})</div></> : <span className="muted">bilinmiyor</span>}</td>
      <td className="small">
        {o.functionalityTested ? <strong className="bad-text">UYARI: test edildi olarak işaretli</strong> : <>Hayır — işlev test edilmedi</>}
        <div className="muted">Kimlik doğrulandı: {o.identityVerified ? "evet" : "hayır"} · Güven: {confidenceLabel(o.confidence)}</div>
      </td>
      <td className="small">
        {o.evidence}
        {o.location && <div className="muted">Yer: {o.location}</div>}
        {o.limitations.length > 0 && (
          <details>
            <summary>Sınırlamalar ({o.limitations.length})</summary>
            <ul>{o.limitations.map((l) => <li key={l}>{l}</li>)}</ul>
          </details>
        )}
      </td>
      <td className="small">
        {formatTime(o.observedAt)}
        <ScanProvenance scan={{ scanId: o.scanId, finishedAt: o.scanFinishedAt, authorizedPath: o.scanAuthorizedPath, authorizedUserConfig: o.scanAuthorizedUserConfig, authorizedProject: o.scanAuthorizedProject }} />
        {o.stale && <div><span className="badge badge-unknown">Eski ({o.ageHours} saat)</span></div>}
        {!o.current && o.notCurrentReason && <div className="muted">{o.notCurrentReason}</div>}
        {o.legacy && <div><span className="badge badge-skipped">Eski kayıt biçimi — gizlilik için konum/ad bilgisi temizlendi</span></div>}
      </td>
    </tr>
  );
}

function ObservationTable({ subject, rows, names, earlier = false }: { subject: string; rows: StoredObservation[]; names: Record<string, string>; earlier?: boolean }) {
  const title = subjectLabel(subject, names);
  const id = `inv-${earlier ? "old-" : ""}${subject}`;
  return (
    <section className="panel" aria-labelledby={id}>
      <h3 id={id}>{title}</h3>
      <div className="table-wrap">
        <table>
          <caption className="sr-only">{title} için {earlier ? "önceki taramalardan gözlemler" : "gözlemler"}</caption>
          <thead>
            <tr>
              <th scope="col">Kapsam / yöntem</th><th scope="col">Bulundu mu?</th><th scope="col">Yapılandırıldı mı?</th><th scope="col">Sürüm</th>
              <th scope="col">İşlev doğrulandı mı?</th><th scope="col">Neden böyle düşünüldü</th><th scope="col">Gözlem zamanı ve kaynağı</th>
            </tr>
          </thead>
          <tbody>{rows.map((o) => <ObservationRow key={o.id} o={o} />)}</tbody>
        </table>
      </div>
    </section>
  );
}

export function InventoryPage() {
  const inventory = useLoad(api.listInventory);
  const projects = useLoad(api.listProjects);
  const catalog = useLoad(api.listCatalog);
  const [pathExec, setPathExec] = useState(false);
  const [userConfig, setUserConfig] = useState(false);
  const [projectOn, setProjectOn] = useState(false);
  const [alias, setAlias] = useState("");
  const [directory, setDirectory] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [done, setDone] = useState("");

  const references = (projects.data ?? []).filter((p) => p.kind === "reference");
  const names = useMemo(() => Object.fromEntries((catalog.data ?? []).map((e) => [e.entryId, e.displayName])), [catalog.data]);
  const projectReady = !projectOn || (alias !== "" && directory.trim() !== "");
  const canScan = (pathExec || userConfig || projectOn) && projectReady && !busy;

  const scan = async () => {
    setBusy(true);
    setError(null);
    setDone("");
    try {
      const s = await api.runDiscoveryScan({ pathExecutables: pathExec, userConfig, project: projectOn ? { alias, directory } : null });
      setDone(`Tarama tamamlandı: ${s.observationCount} gözlem kaydedildi. Hiçbir şey kurulmadı, çalıştırılmadı veya değiştirilmedi.`);
      inventory.reload();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  const data = inventory.data;
  const group = (list: StoredObservation[]) => {
    const groups = new Map<string, StoredObservation[]>();
    for (const o of list) groups.set(o.subject, [...(groups.get(o.subject) ?? []), o]);
    return [...groups.entries()];
  };
  const currentBySubject = useMemo(() => group((data?.observations ?? []).filter((o) => o.current)), [data]);
  const earlierBySubject = useMemo(() => group((data?.observations ?? []).filter((o) => !o.current)), [data]);
  const anyStale = (data?.observations ?? []).some((o) => o.stale);

  return (
    <>
      <div className="banner banner-note" role="note">
        <strong>Bulundu ≠ yapılandırıldı ≠ çalışıyor.</strong> Bu tarama yalnızca <em>okur</em>: dosya adlarına ve belirli yapılandırma dosyalarına bakar.
        Hiçbir program çalıştırılmaz, hiçbir şey kurulmaz, ajan ayarlarınıza yazılmaz, internete bağlanılmaz. Bir dosyanın var olması aracın çalıştığını göstermez;
        işlev doğrulaması bu sürümde yoktur.
      </div>

      <fieldset className="panel">
        <legend>Neye bakılsın? (izin verdiklerinize bakılır)</legend>
        <label className="choice">
          <input type="checkbox" checked={pathExec} onChange={(e) => setPathExec(e.target.checked)} />
          <span>
            <strong>Bilinen program adlarını ara.</strong> PATH klasörlerinde ve Claude Code'un belgelenmiş kurulum konumlarında yalnızca “bu adda bir dosya var mı” diye bakılır.
            Dosyalar açılmaz ve çalıştırılmaz; sürümleri öğrenilmez.
          </span>
        </label>
        <label className="choice">
          <input type="checkbox" checked={userConfig} onChange={(e) => setUserConfig(e.target.checked)} />
          <span>
            <strong>Claude Code kullanıcı dosyalarını oku.</strong> <code>~/.claude/settings.json</code>, <code>~/.claude/plugins/installed_plugins.json</code> ve
            <code>~/.claude.json</code> (yalnızca MCP sunucu tanımlarının varlığı). Şifreler, anahtarlar ve diğer içerik saklanmaz.
          </span>
        </label>
        <label className="choice">
          <input type="checkbox" checked={projectOn} onChange={(e) => setProjectOn(e.target.checked)} />
          <span>
            <strong>Bir proje klasörüne bak.</strong> Yalnızca şu üç dosya okunur: <code>.claude/settings.json</code>, <code>.claude/settings.local.json</code>, <code>.mcp.json</code>.
            Kaynak kodunuza ve diğer dosyalara dokunulmaz.
          </span>
        </label>
        {projectOn && (
          <div className="inline-form">
            <div className="field">
              <label htmlFor="inv-alias">Proje kaydı</label>
              <select id="inv-alias" value={alias} onChange={(e) => setAlias(e.target.value)}>
                <option value="">Seçin</option>
                {references.map((p) => <option key={p.alias} value={p.alias}>{p.displayName}</option>)}
              </select>
              {references.length === 0 && <div className="small muted">Önce Projeler sayfasından gerçek bir proje kaydı ekleyin (örnek projelerin klasörü yoktur).</div>}
            </div>
            <div className="field">
              <label htmlFor="inv-dir">Proje klasörünün tam yolu (yalnızca bu taramada kullanılır, kaydedilmez)</label>
              <input id="inv-dir" type="text" autoComplete="off" spellCheck={false} value={directory} onChange={(e) => setDirectory(e.target.value)} placeholder="C:\Projeler\sitem veya /Users/ad/sitem" />
              <div className="small muted">Ağ paylaşımları, sembolik bağlantılar ve sürücü kökleri reddedilir.</div>
            </div>
          </div>
        )}
        <p>
          <button type="button" onClick={scan} disabled={!canScan}>{data?.lastScan ? "Yeniden tara" : "Bu bilgisayarı tara"}</button>
          <span className="small muted"> En az bir bölüme izin vermeden tarama başlamaz. Tarama kendiliğinden hiç başlamaz.</span>
        </p>
      </fieldset>

      <div aria-live="polite">
        {error && <ErrorNotice error={error} />}
        {done && <p role="status" className="ok-text">✓ {done}</p>}
      </div>
      {inventory.error && <ErrorNotice error={inventory.error} onRetry={inventory.reload} />}

      <section aria-labelledby="h-last">
        <h2 id="h-last">Son tarama</h2>
        {!data?.lastScan ? (
          <p>Henüz hiç tarama yapılmadı. Aşağıdaki hiçbir bilgi bu bilgisayarın gerçek durumunu göstermez.</p>
        ) : (
          <p>
            {formatTime(data.lastScan.finishedAt)} · {platformLabel(data.lastScan.platformTarget)} · {data.lastScan.observationCount} gözlem
            {anyStale && <> <span className="badge badge-unknown">Bazı gözlemler {data.staleAfterHours} saatten eski — yeniden tarayın</span></>}
          </p>
        )}
      </section>

      {currentBySubject.length > 0 && (
        <section aria-labelledby="h-current">
          <h2 id="h-current">Güncel gözlemler (son taramadan)</h2>
          <p className="muted small">Yalnızca son taramada, yetki verilen bölümlerde ve {data?.staleAfterHours} saat içinde görülenler burada gösterilir.</p>
          {currentBySubject.map(([subject, rows]) => <ObservationTable key={subject} subject={subject} rows={rows} names={names} />)}
        </section>
      )}

      {earlierBySubject.length > 0 && (
        <section aria-labelledby="h-earlier" className="panel">
          <h2 id="h-earlier">Önceki taramalardan — güncel değil</h2>
          <p role="note" className="small">
            Bunlar eski sonuçlardır: son taramada bu bölüme bakılmadı, gözlem üretilmedi, tarama {data?.staleAfterHours} saatten eskidir ya da başka bir işletim sistemine aittir.
            Geçmiş olarak saklanır; bugünkü durumu göstermez ve hiçbir işlem için dayanak sayılmaz.
          </p>
          {earlierBySubject.map(([subject, rows]) => <ObservationTable key={subject} subject={subject} rows={rows} names={names} earlier />)}
        </section>
      )}

      {(data?.requests.length ?? 0) > 0 && (
        <section aria-labelledby="h-req">
          <h2 id="h-req">İsteğiniz ile gözlem</h2>
          <p className="muted small">“Etkinleştirmek istiyorum” demeniz aracın kurulu olduğu anlamına gelmez; ikisi ayrı tutulur. Her satır tek bir tercih içindir: araç, proje, ajan yüzeyi ve platform.</p>
          <ul>
            {data!.requests.map((r) => (
              <li key={`${r.entryId}/${r.projectAlias}/${r.agentSurface}/${r.platformTarget}`}>
                <strong>{r.entryName}</strong> · {r.projectName} · {surfaceLabel(r.agentSurface)} · {platformLabel(r.platformTarget)}: Gözlem: <strong>{observedLabel(r.status)}</strong>.{" "}
                <span className="muted">{r.note}</span>
                {r.basis && <ScanProvenance scan={r.basis} />}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section aria-labelledby="h-nc">
        <h2 id="h-nc">Bakılmayanlar</h2>
        <ul>{(data?.notChecked ?? []).map((n) => <li key={n}>{n}</li>)}</ul>
      </section>
    </>
  );
}
