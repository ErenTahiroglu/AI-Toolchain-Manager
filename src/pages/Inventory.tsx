import { useMemo, useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, useLoad } from "../components/common";
import {
  backupStatusLabel, configurationLabel, layerLabel, confidenceLabel, formatTime, methodLabel, notPerformedLabel, obsScopeLabel, observedLabel, platformLabel, presenceLabel, subjectLabel, surfaceLabel,
} from "../labels";
import type { AppError, CleanupPlan, ScanRef, StoredObservation } from "../types";

const permissionText = (p: { authorizedPath: boolean; authorizedUserConfig: boolean; authorizedProject: boolean; authorizedFingerprint: boolean }) => {
  const parts = [
    p.authorizedPath ? "program adları (PATH)" : null,
    p.authorizedUserConfig ? "Claude Code kullanıcı dosyaları" : null,
    p.authorizedProject ? "bir proje klasörü" : null,
    p.authorizedFingerprint ? "dosya parmak izi (SHA-256)" : null,
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


type Step = { key: string; yes: boolean | null; text: string };

const REASONS: Record<string, string> = {
  not_found: "dosya bulunamadı", redirected: "bağlantı/yönlendirme olduğu için okunmadı", non_local: "yerel olmayan birim", not_regular_file: "normal dosya değil",
  access_denied: "erişim izni yok", too_large: "boyut sınırını aşıyor", timed_out: "süre sınırı aşıldı", cancelled: "iptal edildi",
  changed_while_reading: "okunurken değişti", io: "okunamadı", invalid_digest: "geçersiz özet",
};

/** Fingerprint, reference and match as separate lines: a local hash, a self-declared reference and an authenticated one are three different things. */
function artifactSteps(o: StoredObservation): Step[] {
  const a = o.ladder.artifact;
  if (!a) return [];
  const out: Step[] = [];
  out.push(
    a.fingerprint === "computed"
      ? { key: "fingerprint", yes: true, text: `Yerel parmak izi hesaplandı (SHA-256 ${a.sha256?.slice(0, 12) ?? ""}…; yalnızca baytları tanımlar, kaynağı kanıtlamaz)` }
      : a.fingerprint === "unknown"
        ? { key: "fingerprint", yes: null, text: `Parmak izi hesaplanamadı (${REASONS[a.unknownReason ?? ""] ?? "okunamadı"}); sonuç bilinmiyor` }
        : { key: "fingerprint", yes: false, text: "Parmak izi alınmadı (dosya içeriği okunmadı)" },
  );
  if (o.ladder.installRecord || o.ladder.publisher !== "none") out.push({ key: "source", yes: true, text: "Kaynak bilgisi var (yerel kayıt veya paketin kendi beyanı)" });
  out.push(
    a.reference === "authenticated"
      ? { key: "reference", yes: true, text: "Referans özeti var ve bağımsız olarak doğrulanmış" }
      : a.reference === "unauthenticated"
        ? { key: "reference", yes: null, text: "Referans özeti var ama doğrulanmamış (kendi beyanı; güven dayanağı değil)" }
        : { key: "reference", yes: false, text: "Doğrulanmış referans özeti yok" },
  );
  if (a.invalidated) out.push({ key: "invalidated", yes: null, text: "Önceki doğrulama geçersiz: ilke veya referans kümesi değişti" });
  const m: Record<string, string> = {
    matches_authenticated: "Kurulu dosya doğrulanmış referansla eşleşiyor",
    mismatches_authenticated: "Kurulu dosya doğrulanmış referanstan FARKLI",
    matches_unauthenticated: "Yerel özet doğrulanmamış bir referansla eşleşiyor (kanıt değil)",
    mismatches_unauthenticated: "Yerel özet doğrulanmamış referansla uyuşmuyor (sonuç çıkarılmadı)",
    incomparable: "Referans, açılan dosyanın türüne/biçimine bağlanamıyor (arşiv/yükleyici/paket özeti, başka dosya türü veya biçimi, eksik ya da çelişkili kayıt); karşılaştırılamaz",
  };
  if (m[a.matchResult]) out.push({ key: "match", yes: a.matchResult === "matches_authenticated" ? true : a.matchResult === "mismatches_authenticated" ? null : null, text: m[a.matchResult] });
  if (a.recordLink === "unverifiable") out.push({ key: "link", yes: null, text: "Kayıt ile dosya arasındaki bağ doğrulanamadı" });
  if (a.recordLink === "folder_by_name") out.push({ key: "link", yes: null, text: "Kayıt, klasör adıyla eşleşiyor (dosya içeriğiyle değil)" });
  return out;
}

/** Each conclusion is one line; none of them is green, and "functionality" is always untested. */
function ladderSteps(o: StoredObservation): Step[] {
  const l = o.ladder;
  // A version is always labelled with WHO says so; nothing here is an independently validated version.
  const versionText =
    l.version === "known"
      ? l.versionTrust === "validated"
        ? "Sürüm bağımsız olarak doğrulandı"
        : `Sürüm biliniyor (${l.versionTrust === "installer_recorded" ? "yerel kurulum kaydının beyanı" : "paketin kendi beyanı"}; doğrulanmadı)`
      : l.version === "conflicted" ? "Sürüm çelişkili" : "Sürüm bilinmiyor";
  return [
    { key: "detected", yes: o.presence === "present", text: o.presence === "present" ? "Tespit edildi" : "Tespit edilmedi" },
    { key: "configured", yes: o.configuration === "configured", text: o.configuration === "configured" ? "Yapılandırılmış" : "Yapılandırma kanıtı yok" },
    {
      key: "evidence",
      yes: l.identity === "evidence_available" || l.identity === "record_corroborated" || l.identity === "validated",
      text: l.identity === "evidence_available" || l.identity === "record_corroborated" || l.identity === "validated" ? "Kimlik kanıtı mevcut" : "Kimlik kanıtı yok",
    },
    // Partial corroboration is shown as exactly that: a local record that matches, not an authenticated identity.
    ...(l.identity === "record_corroborated"
      ? [{ key: "corroborated", yes: null, text: "Kurulum kaydı katalogla uyuşuyor ve dosyalar görünüyor (yerel, düzenlenebilir kayıt; kurulu içerik doğrulanmadı)" } as Step]
      : []),
    {
      key: "validated",
      yes: l.identity === "validated",
      text: l.identity === "validated" ? "Kimlik doğrulandı (doğrulanmış referans + kurulu dosya eşleşmesi)" : "Kimlik doğrulanmadı",
    },
    ...(l.reclassified ? [{ key: "reclassified", yes: null, text: "Önceki sürümde daha yüksek güvenle kaydedilmişti; kanıtı yetersiz olduğu için düşürüldü" } as Step] : []),
    { key: "version", yes: l.version === "known", text: versionText },
    ...artifactSteps(o),
    { key: "function", yes: false, text: "İşlev test edilmedi" },
    ...(l.identity === "conflicted" || l.version === "conflicted" ? [{ key: "conflict", yes: null, text: "Çelişkili gözlemler var" } as Step] : []),
    ...(o.stale || !o.current ? [{ key: "stale", yes: null, text: "Eski / güncel olmayan gözlem" } as Step] : []),
  ];
}

function Ladder({ o }: { o: StoredObservation }) {
  if (!o.ladder.assessed) {
    return <p className="small muted">Kanıt basamakları bu kayıt için değerlendirilmedi (Phase 2B öncesi tarama).</p>;
  }
  return (
    <>
      <ul className="ladder small" aria-label="Kanıt basamakları">
        {ladderSteps(o).map((st) => (
          <li key={st.key}>
            <span aria-hidden="true">{st.yes === true ? "●" : st.yes === false ? "○" : "◆"}</span> {st.text}{" "}
            <a href={`#ev-${o.id}`} className="muted">(kanıt)</a>
          </li>
        ))}
      </ul>
      {o.ladder.notPerformed.length > 0 && (
        <details>
          <summary className="small">Yapılmayan kontroller ({o.ladder.notPerformed.length})</summary>
          <ul className="small">{o.ladder.notPerformed.map((n) => <li key={n}>{notPerformedLabel(n)}</li>)}</ul>
        </details>
      )}
    </>
  );
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
      <td className="small" id={`ev-${o.id}`}>
        {o.evidence}
        {o.location && <div className="muted">Yer: {o.location}</div>}
        {o.limitations.length > 0 && (
          <details>
            <summary>Sınırlamalar ({o.limitations.length})</summary>
            <ul>{o.limitations.map((l) => <li key={l}>{l}</li>)}</ul>
          </details>
        )}
      </td>
      <td className="small"><Ladder o={o} /></td>
      <td className="small">
        {formatTime(o.observedAt)}
        <ScanProvenance scan={{ scanId: o.scanId, finishedAt: o.scanFinishedAt, authorizedPath: o.scanAuthorizedPath, authorizedUserConfig: o.scanAuthorizedUserConfig, authorizedProject: o.scanAuthorizedProject, authorizedFingerprint: o.scanAuthorizedFingerprint }} />
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
              <th scope="col">İşlev doğrulandı mı?</th><th scope="col">Neden böyle düşünüldü</th><th scope="col">Kanıt basamakları</th><th scope="col">Gözlem zamanı ve kaynağı</th>
            </tr>
          </thead>
          <tbody>{rows.map((o) => <ObservationRow key={o.id} o={o} />)}</tbody>
        </table>
      </div>
    </section>
  );
}


function BackupPanel() {
  const [plan, setPlan] = useState<CleanupPlan | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const look = async () => {
    setBusy(true);
    setError(null);
    try {
      setPlan(await api.previewBackupCleanup());
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="panel" aria-labelledby="h-backups">
      <h2 id="h-backups">Yedekler ve gizlilik</h2>
      <p className="small">
        Veritabanı güncellenmeden önce geri dönüş için bir yedek alınır. Yedek, o andaki verinin birebir kopyasıdır; eski sürümler tam klasör yollarını veya sunucu adlarını
        saklamış olabilir ve bu kopyalar gizlilik düzeltmesinden sonra da <strong>olduğu gibi kalır</strong>. Aşağıdaki düğme yalnızca bakar: hiçbir şeyi silmez, taşımaz, değiştirmez veya göndermez.
      </p>
      <p><button type="button" onClick={look} disabled={busy}>Yedekleri incele</button></p>
      {error && <ErrorNotice error={error} />}
      {plan && (
        <div aria-live="polite">
          <p className="small"><strong>Bu tarama bir örüntü taramasıdır.</strong> “İhlal bulunmadı”, yedeğin kişisel veri içermediği veya silinmesinin güvenli olduğu anlamına gelmez.</p>
          {plan.directoryStatus !== "ok" && plan.directoryStatus !== "missing" && (
            <p role="alert" className="small"><strong>Yedek klasörü güvenli biçimde açılamadı</strong> (bağlantı/junction, ağ yolu veya erişim sorunu); hiçbir yedeğe dokunulmadı.</p>
          )}
          <p className="small muted">
            Tarama sınırları: en çok {plan.limits.maxTextCells.toLocaleString("tr-TR")} metin hücresi, {Math.round(plan.limits.maxRawBytes / 1048576)} MiB ham tarama,
            {" "}{Math.round(plan.limits.maxSqliteImageBytes / 1048576)} MiB veritabanı görüntüsü, {plan.limits.timeoutSeconds} sn. Sınır aşılırsa sonuç “kısmi” olur, “tamamlandı” olmaz.
          </p>
          <details>
            <summary className="small">Neler incelendi, neler incelenmedi?</summary>
            <p className="small"><strong>İncelenen:</strong></p>
            <ul className="small">{plan.examined.map((x) => <li key={x}>{x}</li>)}</ul>
            <p className="small"><strong>İncelenmeyen:</strong></p>
            <ul className="small">{plan.notExamined.map((x) => <li key={x}>{x}</li>)}</ul>
          </details>
          {plan.entries.length === 0 ? <p>Kayıtlı yedek yok.</p> : (
            <div className="table-wrap">
              <table>
                <caption className="sr-only">Veritabanı yedekleri</caption>
                <thead><tr><th scope="col">Yedek</th><th scope="col">Şema</th><th scope="col">Boyut</th><th scope="col">Gizlilik durumu</th><th scope="col">Tarama kapsamı</th><th scope="col">Bulgular (yalnızca sayı)</th></tr></thead>
                <tbody>
                  {plan.entries.map((e) => (
                    <tr key={e.fileName}>
                      <th scope="row" className="small">{e.fileName}</th>
                      <td className="small">v{e.schemaVersion ?? "?"}</td>
                      <td className="small">{Math.max(1, Math.round(e.sizeBytes / 1024))} KB</td>
                      <td className="small">{backupStatusLabel(e.privacyStatus)}</td>
                      <td className="small">
                        {e.coverage === "complete" ? "Tüm katmanlar tamamlandı" : e.coverage === "partial" ? "KISMİ tarama" : "Eksik: bazı katmanlar okunamadı"}
                        <ul>{e.layers.map((l) => <li key={l.layer}>{layerLabel(l)}</li>)}</ul>
                      </td>
                      <td className="small">
                        Gözlem tablosu: {e.observationRowsFailingGate === null ? "incelenemedi" : e.observationRowsFailingGate} · Diğer metin bulgusu: {e.otherTextFindings}
                        {e.tablesWithFindings.length > 0 && <> (tablo: {e.tablesWithFindings.join(", ")})</>} · Ham dosyada yol biçimi: {e.rawPathLikeMatches}
                        {e.hasWalSidecar && <> · -wal dosyası var</>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <ul className="small">
            {plan.preconditions.map((c) => <li key={c.id}>{c.ok ? "Tamam" : "SORUN"}: {c.text}</li>)}
          </ul>
          <ul className="small">{plan.notes.map((n) => <li key={n}>{n}</li>)}</ul>
          <p className="small"><strong>Silme bu sürümde yoktur.</strong> İleride yalnızca sizin açık onayınızla önerilecektir.</p>
        </div>
      )}
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
  const [fingerprint, setFingerprint] = useState(false);
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
      const s = await api.runDiscoveryScan({ pathExecutables: pathExec, userConfig, project: projectOn ? { alias, directory } : null, fingerprintArtifacts: pathExec && fingerprint });
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
          <input type="checkbox" checked={fingerprint && pathExec} disabled={!pathExec} onChange={(e) => setFingerprint(e.target.checked)} />
          <span>
            <strong>Bulunan program dosyalarının parmak izini (SHA-256) hesapla.</strong> Önceki adımda bulunan program dosyasının <em>içeriği okunur</em> ve bir özet hesaplanır; dosya çalıştırılmaz,
            hiçbir yere gönderilmez. Bir özet yalnızca dosyanın baytlarını tanımlar; kaynağını kanıtlamaz. Bunun için önce “program adlarını ara” seçilmelidir.
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
          <p className="muted small">“Bu projede kullanmak istiyorum” demeniz aracın kurulu olduğu anlamına gelmez (bir ajanda etkin olduğu anlamına da gelmez); ikisi ayrı tutulur. Her satır tek bir tercih içindir: araç, proje, ajan yüzeyi ve platform.</p>
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

      <BackupPanel />

      <section aria-labelledby="h-nc">
        <h2 id="h-nc">Bakılmayanlar</h2>
        <ul>{(data?.notChecked ?? []).map((n) => <li key={n}>{n}</li>)}</ul>
      </section>
    </>
  );
}
