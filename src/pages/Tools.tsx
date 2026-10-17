import { api } from "../api";
import { ErrorNotice, UnavailableButton, useLoad } from "../components/common";
import { categoryLabel, compatLabel, installLabel, platformLabel, reviewLabel, surfaceLabel, validationLabel } from "../labels";

export function ToolsPage() {
  const { data, error, loading, reload } = useLoad(api.listTools);

  return (
    <>
      <p className="muted">
        Dokuz aracın <strong>yalnızca açıklama kayıtları</strong>. Hiçbiri kurulu değil, hiçbiri incelenmedi ve hiçbirinin
        çalıştığı doğrulanmadı. "Üretici belirtmiş" ifadesi, üreticinin kendi iddiasıdır; bu uygulama onu test etmedi.
      </p>
      {error && <ErrorNotice error={error} onRetry={reload} />}
      {loading && !data && <p role="status">Yükleniyor…</p>}
      {data?.map((t) => (
        <section key={t.toolId} className="panel" aria-labelledby={`t-${t.toolId}`}>
          <h2 id={`t-${t.toolId}`}>{t.displayName}</h2>
          <p className="small">
            <span className="tag">{categoryLabel(t.category)}</span>{" "}
            <span className="tag">{reviewLabel(t.reviewState)}</span>{" "}
            <span className="tag">{installLabel(t.installState)}</span>{" "}
            <span className="tag">{validationLabel(t.validationState)}</span>
          </p>
          <p className="small muted">
            Kaynak: {t.repoUrl}
            {t.record.license?.spdxId ? ` · Lisans: ${t.record.license.spdxId}` : ""}
            {t.record.sourceSnapshot?.observedRef ? ` · Gözlenen sürüm: ${t.record.sourceSnapshot.observedRef}` : ""}
            {t.record.sourceSnapshot?.immutableCommit === null ? " · Sabit commit bilinmiyor" : ""}
          </p>
          {t.record.privacy && (
            <p className="small">
              Gizlilik (üretici beyanı): telemetri <strong>{t.record.privacy.telemetry ?? "bilinmiyor"}</strong>
              {t.record.privacy.mayTransmitProjectContent === true && " · proje içeriğini aktarabilir"}
              {t.record.privacy.mayTransmitProjectContent === null && " · içerik aktarımı bilinmiyor"}
            </p>
          )}
          {t.record.targetEvidence && (
            <details>
              <summary>Platform ve ajan uyumluluğu ({t.record.targetEvidence.length})</summary>
              <div className="table-wrap">
                <table>
                  <thead><tr><th scope="col">Platform</th><th scope="col">Yüzey</th><th scope="col">Durum</th><th scope="col">Sınırlama</th></tr></thead>
                  <tbody>
                    {t.record.targetEvidence.map((e) => (
                      <tr key={`${e.platformTarget}-${e.agentSurface}`}>
                        <td>{platformLabel(e.platformTarget)}</td>
                        <td>{surfaceLabel(e.agentSurface)}</td>
                        <td>{compatLabel(e.compatibilityState)}</td>
                        <td className="small">{e.limitation}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </details>
          )}
          <p><UnavailableButton label="Kur" reason="Faz 1A'da araç kurulumu yoktur." /></p>
        </section>
      ))}
    </>
  );
}
