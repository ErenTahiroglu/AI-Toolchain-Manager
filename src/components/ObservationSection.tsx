// PRODUCT-04 - the read-only "Son kayıtlı envanter gözlemi" part of one matrix row. It only RENDERS an ObservationView (src/observation.ts); it loads
// nothing, starts no scan and has no button that writes. Raw evidence, locations and file paths are deliberately absent: they stay on the Envanter page.
import { useId } from "react";
import { formatTime, observedLabel, permissionText } from "../labels";
import type { ObservationView } from "../observation";

const NOT_LIVE = "Bu, son kaydedilen tarama sonucudur; şu anki bir denetim değildir. Bu ekran hiçbir tarama başlatmaz ve bilgisayara bakmaz.";

function Main({ view }: { view: ObservationView }) {
  switch (view.kind) {
    case "loading":
      return <p role="status">Kayıtlı envanter okunuyor… Okuma bitene kadar bu araç için hiçbir gözlem gösterilmez.</p>;
    case "unreadable":
      return (
        <p className="bad-text">
          ⚠ Kayıtlı envanter okunamadı; bu araç için gözlem <strong>bilinmiyor</strong>. Bu, “taranmadı” veya “bulunamadı” anlamına gelmez. Hata bildiriminden yeniden deneyebilirsiniz.
        </p>
      );
    case "removed":
      return <p>Bu araç katalogdan kaldırıldı. Kaldırılmış bir kayıt için gözlem karşılaştırması yapılmaz ve güncel bir gözlem varsayılmaz.</p>;
    case "no_wish":
      return (
        <p>
          Bu proje + araç + işletim sistemi + ajan yüzeyi için <strong>kabul edilmiş bir isteğiniz yok</strong>; bu yüzden gözlemle karşılaştırma yapılmadı.
          Bu, aracın bilgisayarda olmadığı ya da hiç taranmadığı anlamına <strong>gelmez</strong>.
        </p>
      );
    case "no_row":
      return (
        <>
          <p>
            İsteğiniz kabul edilmiş görünüyor, ancak okunan envanterde bu <strong>tam hedef</strong> için eşleşen satır yok{view.reading ? " (envanter yeniden okunuyor)" : ""}.
            Bir gözlem varsayılmaz; bu, aracın bulunmadığı anlamına da gelmez. Karar bu envanter okumasından sonra kaydedilmiş olabilir.
          </p>
          {view.old && <p className="bad-text">⚠ Envanter yenilenemedi; elinizdeki okuma eski olabilir.</p>}
        </>
      );
    case "ambiguous":
      return <p className="bad-text">⚠ Envanterde bu tam hedef için birden fazla satır var; çelişkili olduğu için hiçbiri gösterilmiyor.</p>;
    case "unrecognised":
      return (
        <>
          <p>
            Bu uygulama sürümünün tanımadığı bir gözlem durumu geldi (“{view.status.slice(0, 40)}”). Başarı sayılmaz; bundan hiçbir sonuç çıkarılmaz.
          </p>
          {view.old && <p className="bad-text">⚠ Envanter yenilenemedi; bu satır önceki okumadan.</p>}
        </>
      );
    case "observed": {
      const { request } = view;
      const { basis } = request;
      return (
        <>
          <p><strong>{observedLabel(view.status)}</strong></p>
          <p className="muted">{request.note}</p>
          {basis ? (
            <p className="small muted">
              Dayanak tarama: {formatTime(basis.finishedAt)} · taramaya verilen izinler: {permissionText(basis)}
            </p>
          ) : (
            <p className="small muted">Bu durum için bir dayanak tarama kaydı gelmedi.</p>
          )}
          {view.old && <p className="bad-text">⚠ Envanter yenilenemedi; bu, önceki başarılı okumadan ve güncel olmayabilir.</p>}
        </>
      );
    }
  }
}

export function ObservationSection({ view, entryName }: { view: ObservationView; entryName: string }) {
  const uid = useId();
  return (
    <section className="ws-observe" aria-labelledby={`${uid}-h`}>
      <h4 id={`${uid}-h`}>Son kayıtlı envanter gözlemi <span className="sr-only">({entryName})</span></h4>
      <Main view={view} />
      <p className="small muted">{NOT_LIVE}</p>
      <details>
        <summary>Bu gözlem neyi göstermez?</summary>
        <ul className="small">
          <li>Ad eşleşmesi kimlik doğrulaması değildir: aynı adlı başka bir program olabilir.</li>
          <li>“Bulundu”, aracın kurulu, çalışıyor, oturum açılmış veya doğrulanmış olduğu anlamına gelmez; işlev hiç denenmedi.</li>
          <li>Katalogdaki “etkin” işareti, ajanın bu aracı kullandığı anlamına gelmez. İsteğiniz de aracın kurulu olduğunu göstermez.</li>
          <li>Bilgisayar genelindeki bir program adı araması, projeden bağımsız olarak her proje için geçerli sayılabilir; projenin klasörüne bakıldığı anlamına gelmez.</li>
          <li>Ayrıntılı kanıt ve yeniden tarama yalnızca Bilgisayar Envanteri sayfasındadır; orada açık izin vermeden hiçbir şey taranmaz.</li>
        </ul>
      </details>
    </section>
  );
}
