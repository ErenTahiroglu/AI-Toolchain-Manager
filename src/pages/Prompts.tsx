import { api } from "../api";
import { ErrorNotice, UnavailableButton, useLoad } from "../components/common";

export function PromptsPage() {
  const { data, error, loading, reload } = useLoad(api.listPromptSources);

  return (
    <>
      <p className="muted">
        Üç herkese açık istem (prompt) kaynağı <strong>yalnızca planlı referans</strong> olarak listelenir. Bu sürümde hiçbir
        içerik indirilmez, aranmaz, yeniden yazılmaz ve hiçbir ajana gönderilmez. İstem zekâsı özelliği ileride (Faz 4P)
        ve ayrı onayla gelecektir. Kaynaklardaki metinler her zaman <em>güvenilmeyen veri</em> sayılacaktır.
      </p>
      {error && <ErrorNotice error={error} onRetry={reload} />}
      {loading && !data && <p role="status">Yükleniyor…</p>}
      {data?.map((s) => (
        <section key={s.sourceId} className="panel" aria-labelledby={`s-${s.sourceId}`}>
          <h2 id={`s-${s.sourceId}`}>{s.displayName}</h2>
          <p className="small">
            <span className="tag">Planlı</span> <span className="tag">Salt okunur</span>{" "}
            <span className="tag">Güvenilmeyen referans verisi</span>
          </p>
          <dl className="facts">
            <dt>Adres</dt><dd>{s.repoUrl}</dd>
            <dt>Lisans notu</dt><dd>{s.licenseNote}</dd>
            <dt>Planlanan biçim</dt><dd>{s.plannedFormat}</dd>
            <dt>Not</dt><dd>{s.notes}</dd>
          </dl>
          <p><UnavailableButton label="İndir / ara" reason="Faz 4P'ye kadar kullanılamaz." /></p>
        </section>
      ))}
    </>
  );
}
