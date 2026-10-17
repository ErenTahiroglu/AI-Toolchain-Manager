import { useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, StatusBadge, useLoad } from "../components/common";
import { OVERALL, formatTime } from "../labels";
import type { AppError, ChainReport, HealthReport } from "../types";

export function ActivityPage() {
  const dash = useLoad(api.getDashboard);
  const events = useLoad(() => api.listAuditEvents(50));
  const diags = useLoad(() => api.listDiagnostics(50));
  const [report, setReport] = useState<HealthReport | null>(null);
  const [chain, setChain] = useState<ChainReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<AppError | null>(null);

  const shown = report ?? dash.data?.lastReport ?? null;

  const runChecks = async () => {
    setBusy(true);
    setActionError(null);
    try {
      setReport(await api.runHealthChecks());
      events.reload();
      dash.reload();
    } catch (e) {
      setActionError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  const verify = async () => {
    setActionError(null);
    try {
      setChain(await api.verifyAuditChain());
    } catch (e) {
      setActionError(toAppError(e));
    }
  };

  return (
    <>
      <section aria-labelledby="h-checks">
        <h2 id="h-checks">Kendi kendini denetleme</h2>
        <p className="muted">
          Bunlar uygulamanın <strong>kendi iç kontrolleridir</strong> (operasyonel sağlık). Bağımsız bir doğrulama değildir ve
          hiçbir aracı çalıştırmaz. "Bilinmiyor" ve "Atlandı" başarı sayılmaz.
        </p>
        <p><button type="button" onClick={runChecks} disabled={busy}>{busy ? "Çalışıyor…" : "Kontrolleri şimdi çalıştır"}</button></p>
        <div aria-live="polite">
          {actionError && <ErrorNotice error={actionError} />}
          {shown && (
            <>
              <p><StatusBadge status={shown.overall} /> {OVERALL[shown.overall]} <span className="muted small">· {formatTime(shown.ranAt)}</span></p>
              <ul className="checks" aria-label="Kontrol sonuçları">
                {shown.checks.map((c) => (
                  <li key={c.id}>
                    <StatusBadge status={c.status} /> <strong>{c.title}</strong>
                    <div className="small muted">{c.explanation}{c.errorCode && <> <code>{c.errorCode}</code></>}</div>
                  </li>
                ))}
              </ul>
            </>
          )}
          {!shown && <p>Henüz sonuç yok.</p>}
        </div>
      </section>

      <section aria-labelledby="h-audit">
        <h2 id="h-audit">Denetim kaydı</h2>
        <p className="muted small">
          Her kayıt bir öncekine bağlı özetle (hash) zincirlenir; kazara değişiklikleri ve silmeleri yakalar. Aynı bilgisayar
          kullanıcısının kasıtlı yeniden yazmasına karşı <strong>kanıt değildir</strong>.
        </p>
        <p>
          <button type="button" onClick={verify}>Zinciri doğrula</button>{" "}
          {chain && (
            <span role="status">
              <StatusBadge status={chain.ok ? "passed" : "failed"} /> {chain.ok ? `${chain.count} kayıt tutarlı` : chain.problem}
            </span>
          )}
        </p>
        {events.error && <ErrorNotice error={events.error} onRetry={events.reload} />}
        <div className="table-wrap">
          <table>
            <caption className="sr-only">Son 50 denetim kaydı, en yeni üstte</caption>
            <thead><tr><th scope="col">#</th><th scope="col">Zaman</th><th scope="col">Kontrol</th><th scope="col">Sonuç</th><th scope="col">Hata kodu</th></tr></thead>
            <tbody>
              {events.data?.map((e) => (
                <tr key={e.sequence}>
                  <td>{e.sequence}</td><td>{formatTime(e.timestamp)}</td><td>{e.checkId ?? e.eventType}</td>
                  <td><StatusBadge status={e.outcome} /></td><td>{e.errorCode ?? ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section aria-labelledby="h-diag">
        <h2 id="h-diag">Yerel tanılama kayıtları</h2>
        <p className="muted small">Yalnızca bu bilgisayarda tutulur; gizli değerler ve kişisel yollar silinir; hiçbir yere gönderilmez.</p>
        {diags.error && <ErrorNotice error={diags.error} onRetry={diags.reload} />}
        <div className="table-wrap">
          <table>
            <caption className="sr-only">Son 50 tanılama kaydı, en yeni üstte</caption>
            <thead><tr><th scope="col">Zaman</th><th scope="col">Düzey</th><th scope="col">Bileşen</th><th scope="col">Kod</th><th scope="col">Ayrıntı</th></tr></thead>
            <tbody>
              {diags.data?.map((d) => (
                <tr key={d.id}>
                  <td>{formatTime(d.ts)}</td><td>{d.level}</td><td>{d.component}</td><td><code>{d.code}</code></td><td>{d.detail}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {diags.data?.length === 0 && <p className="muted">Kayıt yok.</p>}
      </section>
    </>
  );
}
