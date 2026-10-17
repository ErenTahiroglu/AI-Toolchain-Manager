import { api } from "../api";
import { StatusBadge, ErrorNotice, useLoad } from "../components/common";
import { OVERALL, formatTime } from "../labels";

export function DashboardPage({ onNavigate }: { onNavigate: (page: string) => void }) {
  const { data, error, loading, reload } = useLoad(api.getDashboard);

  if (error) return <ErrorNotice error={error} onRetry={reload} />;
  if (loading || !data) return <p role="status">Yükleniyor…</p>;

  const report = data.lastReport;
  return (
    <>
      <section aria-labelledby="h-health">
        <h2 id="h-health">Uygulamanın kendi sağlık kontrolü</h2>
        {report ? (
          <p>
            <StatusBadge status={report.overall} /> {OVERALL[report.overall]}
            <span className="muted small"> · Son kontrol: {formatTime(report.ranAt)}</span>
          </p>
        ) : (
          <p><StatusBadge status="unknown" /> Henüz sağlık kontrolü çalıştırılmadı.</p>
        )}
        <p className="muted small">
          Bu yalnızca uygulamanın kendi iç denetimidir (veritabanı, kayıt bütünlüğü, izinler…). Hiçbir araç çalıştırılmadı,
          bu yüzden <strong>hiçbir aracın gerçekten çalıştığı doğrulanmış değildir</strong>.{" "}
          <button type="button" className="link" onClick={() => onNavigate("activity")}>Ayrıntılar</button>
        </p>
      </section>

      <section aria-labelledby="h-numbers">
        <h2 id="h-numbers">Özet</h2>
        <dl className="cards">
          <div className="card"><dt>Katalogdaki araç</dt><dd>{data.toolCount}</dd></div>
          <div className="card"><dt>İncelenmemiş araç</dt><dd>{data.unreviewedCount}</dd></div>
          <div className="card"><dt>Kurulu / yönetilen araç</dt><dd>{data.installedCount}</dd></div>
          <div className="card"><dt>Doğrulanmış araç</dt><dd>{data.validatedCount}</dd></div>
          <div className="card"><dt>Örnek proje</dt><dd>{data.sampleProjectCount}</dd></div>
          <div className="card"><dt>İstem kaynağı (planlı)</dt><dd>{data.promptSourceCount}</dd></div>
        </dl>
        <p className="muted small">
          Bu sürüm (Faz 1A) hiçbir aracı kurmaz, hiçbir ajan ayarını değiştirmez ve internete istek göndermez.
          Projeler ekranındaki projeler <strong>örnek veridir</strong>.
        </p>
      </section>

      <section aria-labelledby="h-audit">
        <h2 id="h-audit">Denetim kaydı</h2>
        <p>
          <StatusBadge status={data.auditChain.ok ? "passed" : "failed"} /> {data.auditEventCount} kayıt.{" "}
          {data.auditChain.ok ? "Zincir tutarlı." : data.auditChain.problem}
        </p>
      </section>
    </>
  );
}
