import { useEffect, useRef, useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, useLoad } from "./common";
import {
  INSTALL_STATUS_HELP, changeLabel, formatTime, installStatusLabel, licenseLabel, platformLabel, resultLabel, rollbackStatusLabel,
} from "../labels";
import type { AppError, InstallAction, InstallPreview, InstallResult, ReceiptView, ToolState } from "../types";

/**
 * The installation workflow of one tool: status -> preview -> confirmation -> result -> installed details.
 *
 * Everything shown here comes from Rust; the only things this component sends back are identifiers (entry id, offer id), the digest of the
 * preview the person was looking at, and the acknowledgement. There is no path, URL, command line or byte anywhere in the requests.
 */

const ICON: Record<string, string> = {
  verification_incomplete: "?", license_review_required: "§", available: "✓", ready_for_approval: "⏳", installing: "…",
  installed_verified: "✓", failed_needs_attention: "✕", removed: "–",
};

export function StatusTag({ status }: { status: ToolState["status"] }) {
  return (
    <span className={`tag install-status install-${status}`}>
      <span aria-hidden="true">{ICON[status] ?? "•"}</span> {installStatusLabel(status)}
    </span>
  );
}

const shortDigest = (d: string) => `${d.slice(0, 12)}…${d.slice(-8)}`;

function Receipt({ r }: { r: ReceiptView }) {
  return (
    <dl className="facts small">
      <dt>İşlem</dt><dd>{r.action === "install" ? "Kurulum" : r.action === "uninstall" ? "Kaldırma" : "Güncelleme"}</dd>
      <dt>Sürüm</dt><dd>{r.version ?? "—"}</dd>
      <dt>Tamamlandı</dt><dd>{formatTime(r.completedAt)}</dd>
      <dt>İşlem kimliği</dt><dd><code>{r.txnId}</code></dd>
      {r.artifactSha256 && (<><dt>Dosya özeti (SHA-256)</dt><dd><code>{r.artifactSha256}</code></dd></>)}
      {r.kept.length > 0 && (<><dt>Dokunulmayanlar</dt><dd>{r.kept.includes("user_data") ? "Aracın kendi kullanıcı verisi" : r.kept.join(", ")}</dd></>)}
    </dl>
  );
}

function useRemaining(expiresAt: string) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return Math.max(0, Math.floor((new Date(expiresAt).getTime() - now) / 1000));
}

function PreviewView({
  preview, busy, onConfirm, onCancel,
}: { preview: InstallPreview; busy: boolean; onConfirm: (acknowledged: boolean) => void; onCancel: () => void }) {
  const c = preview.content;
  const [read, setRead] = useState(false);
  const remaining = useRemaining(preview.expiresAt);
  const expired = remaining <= 0;
  const removal = c.action === "uninstall";
  return (
    <section className="preview-card" aria-labelledby="h-preview">
      <h3 id="h-preview">{removal ? "Kaldırma önizlemesi" : "Kurulum önizlemesi"}: {c.displayName} {c.version}</h3>
      <p className="banner banner-note small" role="note">
        Bu bir <strong>önizlemedir</strong>: şu ana kadar hiçbir şey değişmedi. Aşağıdaki onayı vermezseniz hiçbir şey olmaz.
        Hiçbir program çalıştırılmaz.
      </p>
      {c.synthetic && (
        <div className="banner banner-synthetic" role="note"><strong>SENTETİK DENEME</strong> — gerçek bir program değildir; yalnızca akışı denemek içindir.</div>
      )}

      <h4>Ne kurulacak?</h4>
      <dl className="facts small">
        <dt>Araç</dt><dd>{c.displayName} (<code>{c.entryId}</code>)</dd>
        <dt>Sürüm</dt><dd>{c.version}</dd>
        <dt>Platform / mimari</dt><dd>{platformLabel(c.platformTarget)} · {c.architecture}</dd>
        <dt>Kaynak depo</dt><dd>{c.source.repository}</dd>
        <dt>Sabit commit</dt><dd><code>{c.source.immutableCommit}</code> {c.source.sourceAttested ? "(bağımsız kanıtla bağlandı)" : "(yalnızca beyan; doğrulanmadı)"}</dd>
        <dt>Dosya</dt><dd>{c.artifact.fileName} · {c.artifact.sizeBytes} bayt · {c.artifact.format}</dd>
        <dt>Dosya özeti (SHA-256)</dt><dd><code>{c.artifact.sha256}</code></dd>
        <dt>Dosyanın kaynağı</dt><dd>{c.originText}</dd>
        <dt>Özetin kapsamı</dt><dd>Kurulacak dosyanın kendisi (arşiv değil)</dd>
        <dt>Doğrulama</dt><dd>Dosyanın özeti, bağımsız olarak doğrulanmış bir referans özetiyle ve tarifle eşleşti.</dd>
        <dt>Lisans</dt><dd>{c.licenseState === "accepted" ? "Bir incelemeyle kabul edildi" : c.licenseState === "accepted_local_use" ? "Yalnızca bu bilgisayardaki yerel kullanım için kabul edildi (yeniden dağıtım veya paketleme için DEĞİL)" : licenseLabel(c.licenseState)} · {c.licenseSpdx}</dd>
        <dt>Kaynak doğrulaması</dt><dd>{c.source.sourceAttested ? "Kaynak, bağımsız bir kanıt kaydıyla depo, iş akışı ve commit'e bağlandı" : "Yalnızca beyan; kanıtlanmadı"}</dd>
        <dt>Lisansın kapsamı</dt><dd>{c.licenseScopeText}</dd>
        <dt>Kurulan dosya nasıl saklanır?</dt><dd>{c.installedAsText}</dd>
        <dt>Ağ</dt><dd>{c.networkUsed ? "Ağ kullanılacak" : "Ağ kullanılmayacak; hiçbir şey indirilmeyecek"}</dd>
      </dl>

      {c.notices && (
        <>
          <h4>Üçüncü taraf bildirimleri</h4>
          <p className="small">
            {c.notices.title} · özet <code>{shortDigest(c.notices.sha256)}</code>. Bildirimler onayınıza <strong>dahildir</strong>: farklı bir bildirim metni
            farklı bir onay sayılır. Tam metin kurulumdan sonra araç ayrıntılarında okunabilir.
          </p>
          <ul className="small">{c.notices.components.map((x) => <li key={x}>{x}</li>)}</ul>
          {c.notices.unresolved.length > 0 && (
            <>
              <p className="small"><strong>Çözülmemiş hukuki noktalar:</strong></p>
              <ul className="small">{c.notices.unresolved.map((x) => <li key={x}>{x}</li>)}</ul>
            </>
          )}
        </>
      )}

      <h4>Dosya sisteminde ne olacak?</h4>
      {!c.managerFolderExists && !removal && <p className="small">Bu ilk kurulum: yöneticinin kendi klasörü oluşturulacak.</p>}
      <div className="table-wrap">
        <table aria-label="Dosya sistemi etkileri">
          <thead><tr><th scope="col">Yer (yöneticinin klasörüne göre)</th><th scope="col">Ne olur</th><th scope="col">İşlem sonunda duruyor mu?</th><th scope="col">Açıklama</th></tr></thead>
          <tbody>
            {c.effects.map((e) => (
              <tr key={`${e.place}-${e.change}`}>
                <td><code>{e.place}</code></td><td>{changeLabel(e.change)}</td><td>{e.change === "read" ? "—" : e.remains ? "Evet" : "Hayır (geçici)"}</td><td className="small">{e.text}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <h4>Yapılmayacaklar</h4>
      <ul className="small" aria-label="Yapılmayacaklar">{c.notDone.map((x) => <li key={x}>{x}</li>)}</ul>

      <h4>Geri alma</h4>
      <p className="small">{c.rollback.text}</p>
      <p className="small"><strong>Geri alınanlar:</strong> {c.rollback.undoes.join(", ")}. <strong>Geri alınmayanlar:</strong> {c.rollback.doesNotUndo.join(", ")}.</p>

      <h4>Bu onay nedir?</h4>
      <p className="small" role="note">{c.consentStatement}</p>

      <p className="small muted">
        Önizleme kimliği <code>{shortDigest(preview.previewDigest)}</code> · <span role="timer" aria-live="off">{expired ? "Süre doldu" : `Kalan süre: ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, "0")}`}</span>
      </p>
      {expired && <p role="alert" className="bad-text">Önizlemenin süresi doldu. Onay artık kabul edilmez; önizlemeyi yeniden açın.</p>}
      <label className="choice">
        <input type="checkbox" checked={read} disabled={busy || expired} onChange={(e) => setRead(e.target.checked)} />
        Yukarıdaki tüm etkileri okudum ve anladım.
      </label>
      <p className="actions">
        <button type="button" disabled={!read || busy || expired} onClick={() => onConfirm(read)}>
          {removal ? "Onaylıyorum ve kaldır" : "Onaylıyorum ve kur"}
        </button>
        <button type="button" className="secondary" disabled={busy} onClick={onCancel}>Vazgeç</button>
      </p>
    </section>
  );
}

function ResultView({ result, onClose, onDetails }: { result: InstallResult; onClose: () => void; onDetails: () => void }) {
  const ok = result.outcome === "completed" || result.outcome === "already_applied";
  const happened = ok || result.outcome === "needs_attention";
  return (
    <section className="result-card" aria-labelledby="h-result">
      <h3 id="h-result">Sonuç: {resultLabel(result.outcome)}</h3>
      <p role="status" className={ok ? "ok-text" : "bad-text"}>{ok ? "✓" : "✕"} {result.message}</p>
      {!result.auditRecorded && <p className="small bad-text" role="note">Bu cevabın denetim kaydı yazılamadı; gerçek durum ve makbuz doğrudur.</p>}
      {result.code && <p className="small muted">Kod: <code>{result.code}</code></p>}
      {result.receipt && (<><h4>Makbuz</h4><Receipt r={result.receipt} /></>)}
      {result.rollback && (
        <>
          <h4>Geri alma</h4>
          <p className="small"><strong>{rollbackStatusLabel(result.rollback.status)}</strong></p>
          {result.rollback.undone.length > 0 && (<><p className="small">Geri alınanlar:</p><ul className="small">{result.rollback.undone.map((x) => <li key={x}>{x}</li>)}</ul></>)}
          {result.rollback.notUndone.length > 0 && (
            <><p className="small bad-text">Geri ALINAMAYANLAR (hiçbiri silinmedi veya tahmin edilmedi):</p><ul className="small">{result.rollback.notUndone.map((x) => <li key={x}>{x}</li>)}</ul></>
          )}
          {result.rollback.notCovered.length > 0 && (<><p className="small">Geri almanın kapsamı dışında kalanlar:</p><ul className="small">{result.rollback.notCovered.map((x) => <li key={x}>{x}</li>)}</ul></>)}
        </>
      )}
      <p className="actions">
        {happened && result.state && (result.state.status === "installed_verified" || result.state.status === "removed" || result.state.status === "failed_needs_attention") && (
          <button type="button" onClick={onDetails}>Araç ayrıntılarını göster</button>
        )}
        <button type="button" className="secondary" onClick={onClose}>Tamam</button>
      </p>
    </section>
  );
}

function DetailsView({ entryId, onClose }: { entryId: string; onClose: () => void }) {
  const { data, error, loading } = useLoad(() => api.getInstalledDetails(entryId), [entryId]);
  if (error) return <ErrorNotice error={error} />;
  if (loading || !data) return <p role="status">Yükleniyor…</p>;
  return (
    <section className="details-card" aria-labelledby="h-details">
      <h3 id="h-details">Kurulu araç ayrıntıları: {data.displayName}</h3>
      <p><StatusTag status={data.state.status} /> <span className="small muted">{INSTALL_STATUS_HELP[data.state.status]}</span></p>
      {data.version ? (
        <dl className="facts small">
          <dt>Kurulu sürüm</dt><dd>{data.version}</dd>
          <dt>Konum (yöneticinin klasörüne göre)</dt><dd><code>{data.location}</code></dd>
          {data.artifact && (<><dt>Dosya</dt><dd>{data.artifact.fileName} · {data.artifact.sizeBytes} bayt</dd><dt>Dosya özeti (SHA-256)</dt><dd><code>{data.artifact.sha256}</code></dd></>)}
          {data.kept.length > 0 && (<><dt>Dokunulmayanlar</dt><dd>Aracın kendi kullanıcı verisi</dd></>)}
        </dl>
      ) : (
        <p className="small">Şu anda kurulu bir sürüm yok.</p>
      )}
      {data.checks.length > 0 && (
        <>
          <h4>Doğrulama (şu an yapıldı)</h4>
          <ul className="checks small" aria-label="Doğrulama kontrolleri">
            {data.checks.map((k) => <li key={k.text}><span aria-hidden="true">{k.passed ? "✓" : "✕"}</span> {k.passed ? "Geçti" : "BAŞARISIZ"}: {k.text}</li>)}
          </ul>
        </>
      )}
      {data.receipt && (<><h4>Son makbuz</h4><Receipt r={data.receipt} /></>)}
      {data.history.length > 0 && (
        <details>
          <summary>İşlem geçmişi ({data.history.length})</summary>
          <ol className="small">{data.history.map((h) => <li key={h.txnId}>{h.action === "install" ? "Kurulum" : h.action === "uninstall" ? "Kaldırma" : "Güncelleme"} · {h.version ?? "—"} · {formatTime(h.completedAt)}</li>)}</ol>
        </details>
      )}
      {data.notices && (
        <details>
          <summary>Üçüncü taraf bildirimleri: {data.notices.title}</summary>
          <p className="small">Özet <code>{data.notices.sha256}</code></p>
          <ul className="small">{data.notices.components.map((x) => <li key={x}>{x}</li>)}</ul>
          {data.notices.unresolved.length > 0 && (<><p className="small"><strong>Çözülmemiş hukuki noktalar:</strong></p><ul className="small">{data.notices.unresolved.map((x) => <li key={x}>{x}</li>)}</ul></>)}
          <pre className="notice-text">{data.notices.text}</pre>
        </details>
      )}
      <details>
        <summary>Bu uygulamanın yapmadıkları</summary>
        <ul className="small">{data.never.map((x) => <li key={x}>{x}</li>)}</ul>
      </details>
      <p className="actions"><button type="button" className="secondary" onClick={onClose}>Kapat</button></p>
    </section>
  );
}

type View =
  | { kind: "idle" }
  | { kind: "preview"; preview: InstallPreview }
  | { kind: "applying"; preview: InstallPreview }
  | { kind: "result"; result: InstallResult }
  | { kind: "details" };

export function InstallPanel({ entryId, state, onChanged }: { entryId: string; state: ToolState | undefined; onChanged: () => void }) {
  const [view, setView] = useState<View>({ kind: "idle" });
  const [error, setError] = useState<AppError | null>(null);
  const [recovery, setRecovery] = useState("");
  const openOffer = useRef<string | null>(null);
  openOffer.current = view.kind === "preview" ? view.preview.offerId : null;

  // closing the screen without confirming withdraws the offer
  useEffect(() => () => { if (openOffer.current) api.cancelInstall(openOffer.current).catch(() => undefined); }, []);

  if (!state) return null;

  const open = async (action: InstallAction) => {
    setError(null);
    setRecovery("");
    try {
      const preview = await api.prepareInstall(entryId, action);
      setView({ kind: "preview", preview });
      onChanged();
    } catch (e) {
      setError(toAppError(e));
    }
  };

  const confirm = async (preview: InstallPreview, acknowledged: boolean) => {
    setError(null);
    setView({ kind: "applying", preview });
    try {
      const result = await api.confirmInstall(preview.offerId, preview.previewDigest, acknowledged);
      setView({ kind: "result", result });
    } catch (e) {
      setError(toAppError(e));
      setView({ kind: "idle" });
    } finally {
      onChanged();
    }
  };

  const cancel = async (preview: InstallPreview) => {
    setError(null);
    try {
      await api.cancelInstall(preview.offerId);
    } catch (e) {
      setError(toAppError(e));
    }
    setView({ kind: "idle" });
    onChanged();
  };

  const recover = async () => {
    setError(null);
    try {
      const r = await api.recoverInstallations();
      setRecovery(
        r.examined
          ? `Denetlendi: ${r.rolledBack} işlem geri alındı, ${r.rolledForward} işlem tamamlandı, ${r.needsAttention} işlem hâlâ dikkat gerektiriyor; ${r.auditReconciled} eksik denetim kaydı makbuzdan yeniden kuruldu, ${r.auditUnreconciled} kayıt kurulamadı.${r.notes.length ? " " + r.notes.join(" ") : ""}`
          : "Denetlenecek bir şey yoktu.",
      );
    } catch (e) {
      setError(toAppError(e));
    }
    onChanged();
  };

  const idle = view.kind === "idle";
  const canOpenInstall = idle && state.canInstall && (state.status === "available" || state.status === "removed" || state.status === "ready_for_approval");
  const canOpenRemoval = idle && state.canRemove && (state.status === "installed_verified" || state.status === "ready_for_approval");
  const needsRecovery = state.status === "failed_needs_attention" && state.reasons.some((r) => r.code === "unfinished_transaction" || r.code === "audit_outcome_missing");

  return (
    <div className="install-panel" role="group" aria-label={`Kurulum: ${entryId}`}>
      <h3>Kurulum</h3>
      <p>
        <StatusTag status={state.status} />{" "}
        {state.version && <span className="small muted">Sürüm {state.installedVersion ?? state.version}</span>}
        {state.synthetic && <> <span className="tag">SENTETİK deneme</span></>}
      </p>
      <p className="small muted">{INSTALL_STATUS_HELP[state.status]}</p>
      {state.reasons.length > 0 && (
        <ul className="small" aria-label="Nedenler">{state.reasons.map((r) => <li key={r.code}>{r.text} <code>{r.code}</code></li>)}</ul>
      )}
      {error && <ErrorNotice error={error} />}
      {recovery && <p role="status" className="small">{recovery}</p>}

      {view.kind === "preview" && (
        <PreviewView preview={view.preview} busy={false} onConfirm={(a) => confirm(view.preview, a)} onCancel={() => cancel(view.preview)} />
      )}
      {view.kind === "applying" && (
        <p role="status" aria-live="polite" className="banner banner-note">
          {view.preview.content.action === "uninstall" ? "Kaldırılıyor…" : "Kuruluyor…"} Lütfen bekleyin; işlem sürerken bu pencereyi kapatmayın.
        </p>
      )}
      {view.kind === "result" && (
        <ResultView result={view.result} onClose={() => setView({ kind: "idle" })} onDetails={() => setView({ kind: "details" })} />
      )}
      {view.kind === "details" && <DetailsView entryId={entryId} onClose={() => setView({ kind: "idle" })} />}

      {idle && (
        <p className="actions">
          {canOpenInstall && <button type="button" onClick={() => open("install")}>Kurulum önizlemesini aç</button>}
          {canOpenRemoval && <button type="button" onClick={() => open("uninstall")}>Kaldırma önizlemesini aç</button>}
          {(state.status === "installed_verified" || state.status === "removed" || state.status === "failed_needs_attention") && (
            <button type="button" className="secondary" onClick={() => setView({ kind: "details" })}>Araç ayrıntıları</button>
          )}
          {needsRecovery && <button type="button" onClick={recover}>Kurtarmayı çalıştır</button>}
        </p>
      )}
    </div>
  );
}
