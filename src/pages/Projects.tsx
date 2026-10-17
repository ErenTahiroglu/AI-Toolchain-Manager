import { api } from "../api";
import { ErrorNotice, SyntheticBanner, UnavailableButton, useLoad } from "../components/common";
import { decisionLabel, platformLabel, surfaceLabel } from "../labels";

export function ProjectsPage() {
  const { data, error, loading, reload } = useLoad(api.listProjects);

  return (
    <>
      <SyntheticBanner>
        Aşağıdaki projeler uydurma örneklerdir; bilgisayarınızdaki gerçek klasörlerle ilgisi yoktur. Kararlar yalnızca
        arayüzü göstermek içindir, hiçbir şey kurulmaz veya değiştirilmez.
      </SyntheticBanner>
      <p><UnavailableButton label="Proje ekle" reason="Faz 1A'da gerçek proje eklenemez." /></p>
      {error && <ErrorNotice error={error} onRetry={reload} />}
      {loading && !data && <p role="status">Yükleniyor…</p>}
      {data?.map((p) => (
        <section key={p.alias} className="panel" aria-labelledby={`p-${p.alias}`}>
          <h2 id={`p-${p.alias}`}>{p.displayName} <span className="tag">örnek</span></h2>
          <p className="muted">{p.description}</p>
          <div className="table-wrap">
            <table>
              <caption className="sr-only">{p.displayName} için örnek araç kararları</caption>
              <thead>
                <tr><th scope="col">Araç</th><th scope="col">Ajan yüzeyi</th><th scope="col">Platform</th><th scope="col">Örnek karar</th><th scope="col">Gerçek durum</th></tr>
              </thead>
              <tbody>
                {p.decisions.map((d) => (
                  <tr key={`${d.toolId}-${d.agentSurface}-${d.platformTarget}`}>
                    <th scope="row">{d.toolName}</th>
                    <td>{surfaceLabel(d.agentSurface)}</td>
                    <td>{platformLabel(d.platformTarget)}</td>
                    <td>{decisionLabel(d.decision)}</td>
                    <td>Bilinmiyor (hiçbir şey denetlenmedi)</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      ))}
    </>
  );
}
