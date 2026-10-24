// PRODUCT-05 - the Dashboard's small window onto the review queue. It uses the same pure derivation and the same headline sentence as the queue on the
// Projects page (so the two cannot disagree), two existing read-only loads (projects, STORED inventory) and one link. It never shows a bare "0" while
// either read is unfinished or failed, and it has no way to mark anything as seen, and none to save or scan.
import { useMemo } from "react";
import { api } from "../api";
import { deriveReview, reviewHeadline } from "../review";
import { useLoad } from "./common";

export function ReviewSummary({ onNavigate }: { onNavigate: (page: string) => void }) {
  const projects = useLoad(api.listProjects);
  const inventory = useLoad(api.listInventory);
  const now = useMemo(() => Date.now(), [projects.data, inventory.data]); // eslint-disable-line react-hooks/exhaustive-deps
  const queue = useMemo(() => deriveReview(projects, inventory, now), [projects, inventory, now]);
  const unreliable = queue.projects === "failed" || queue.projects === "old" || queue.evidence === "failed" || queue.evidence === "old";

  return (
    <section aria-labelledby="h-review">
      <h2 id="h-review">İncelenmesi gerekenler</h2>
      <p role="status">{reviewHeadline(queue)}</p>
      {queue.complete && queue.uncertain.length > 0 && (
        <p className="small bad-text">⚠ {queue.uncertain.length} karar için kayıtlı kanıt belirsiz; ayrıntı Projeler sayfasında.</p>
      )}
      {(queue.projects === "refreshing" || queue.evidence === "refreshing") && <p className="small bad-text">⚠ Okuma sürüyor; yukarıdaki sayı kesin değil.</p>}
      {queue.projects !== "loading" && queue.projects !== "failed" && (queue.evidence === "failed" || queue.evidence === "old") && (
        <p className="small bad-text">⚠ Kayıtlı envanter okunamadı veya yenilenemedi; gözlemden doğan nedenler eksik ya da eski olabilir.</p>
      )}
      <p className="muted small">
        Yalnızca kayıtlı kararlarınızdan, katalog işaretlerinden, hatırlatma tarihlerinden ve son <strong>kayıtlı</strong> envanter sonucundan türetilir; hiçbir şey taramaz, kaydetmez veya kurmaz.
        Bir öğeyi görmek sayıyı azaltmaz.{" "}
        {unreliable && <button type="button" className="secondary" onClick={() => { projects.reload(); inventory.reload(); }}>Yeniden oku</button>}{" "}
        <button type="button" className="link" onClick={() => onNavigate("projects")}>İnceleme listesini aç</button>
      </p>
    </section>
  );
}
