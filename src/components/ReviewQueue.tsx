// PRODUCT-05 - "İncelenmesi Gerekenler": a read-only list of saved decisions that deserve a second look (see src/review.ts for the rules). It decides,
// saves, resets, evaluates, scans and installs nothing. Its only action is "Matriste incele", which hands the exact target + tool to the Project Tool
// Workspace through the existing typed handoff; every decision stays in the matrix. It reads the loads the Projects page already has: no new IPC.
import { useId, useMemo, useState } from "react";
import { formatTime, observedLabel, platformLabel, surfaceLabel } from "../labels";
import { deriveReview, filterItems, reviewHeadline, type ReasonFilter, type ReviewItem, type ReviewReason, type UncertainReason } from "../review";
import type { Inventory, ProjectView } from "../types";
import { workspaceDecisionLabel, type TargetRequest } from "../workspace";
import { EvidenceOriginNote } from "./EvidenceOriginNote";
import type { LoadState } from "./ProjectToolWorkspace";

const EVIDENCE_GLOSS: Record<string, string> = {
  not_scanned: "Bu hedef için kayıtlı envanterde henüz gözlem yok; bu, aracın olmadığı anlamına gelmez.",
  absent_current: "İsteğiniz kabul edilmiş, ancak son kayıtlı taramada bulunamadı.",
  unknown_current: "İsteğiniz kabul edilmiş; son kayıtlı taramada bulunup bulunmadığı belirlenemedi.",
  previously_present: "Kayıtlı gözlem eski; bugünü göstermez.",
  previously_absent: "Kayıtlı gözlem eski; bugünü göstermez.",
  previously_unknown: "Kayıtlı gözlem eski; bugünü göstermez.",
  platform_mismatch: "Kayıtlı gözlemler yalnızca başka bir işletim sistemi içindir; bu hedef için gözlem yok.",
};

function reasonText(r: ReviewReason, item: ReviewItem): string {
  switch (r.code) {
    case "material_change": return "Karardan sonra aracın yetenekleri veya uyumluluğu katalogda önemli ölçüde değişti.";
    case "entry_removed": return "Araç katalogdan kaldırıldı; eski karar duruyor (matriste yalnızca görülebilir veya sıfırlanabilir).";
    case "reminder_due": return `Hatırlatma tarihi geldi (${formatTime(r.remindAt)}); karar hâlâ “Şimdi değil”.`;
    case "evidence": {
      const scan = item.observation?.scanFinishedAt ? ` Dayanak tarama: ${formatTime(item.observation.scanFinishedAt)}.` : "";
      return `Kayıtlı gözlem: ${observedLabel(r.status)}. ${EVIDENCE_GLOSS[r.status] ?? ""}${scan}`.trim();
    }
  }
}

const UNCERTAIN_TEXT: Record<UncertainReason, string> = {
  reminder_unreadable: "Hatırlatma tarihi okunamadı; zamanının gelip gelmediği bilinmiyor.",
  no_row: "Kayıtlı envanterde bu kabul edilmiş istek için eşleşen satır yok; gözlem bilinmiyor.",
  ambiguous: "Kayıtlı envanterde aynı hedef için birden fazla satır var; hiçbiri gösterilmiyor.",
  unrecognised_status: "Kayıtlı envanterde bu uygulamanın tanımadığı bir durum var; başarı sayılmaz, bir sonuç çıkarılmaz.",
  duplicate_wish: "Aynı hedef için birden fazla karar kaydı var; yalnızca ilki gösteriliyor.",
};

const REASON_FILTERS: { value: ReasonFilter; label: string }[] = [
  { value: "all", label: "Tümü" },
  { value: "material_change", label: "Önemli değişiklik" },
  { value: "entry_removed", label: "Katalogdan kaldırılmış" },
  { value: "reminder_due", label: "Hatırlatma tarihi geldi" },
  { value: "evidence", label: "Kayıtlı gözlem" },
];

function Item({ item, onOpen }: { item: ReviewItem; onOpen: (r: TargetRequest & { entryId: string }) => void }) {
  const where = `${item.projectName} · ${platformLabel(item.platform)} · ${surfaceLabel(item.surface)}`;
  return (
    <li className="rq-item">
      <p>
        <strong>{item.entryName}</strong> <code className="small">{item.entryId}</code>
        <span className="muted"> — {where}{item.synthetic ? " (örnek proje: uydurma veri)" : ""}</span>
      </p>
      <p className="small">
        İsteğiniz: <strong>{workspaceDecisionLabel(item.decision)}</strong>
        <span className="muted"> (kayıtlı istek; kurulu, doğrulanmış veya bir ajanda etkin olduğu anlamına gelmez)</span>
      </p>
      {item.reasons.length > 0 && <ul className="small">{item.reasons.map((r) => <li key={r.code}>{reasonText(r, item)}</li>)}</ul>}
      {item.uncertain.length > 0 && (
        <ul className="small">{item.uncertain.map((u) => <li key={u} className="bad-text">⚠ Kanıt belirsiz: {UNCERTAIN_TEXT[u]}</li>)}</ul>
      )}
      {item.observation && !item.reasons.some((r) => r.code === "evidence") && (
        <p className="small muted">
          Kayıtlı gözlem: {observedLabel(item.observation.status)}
          {item.observation.scanFinishedAt ? ` · dayanak tarama: ${formatTime(item.observation.scanFinishedAt)}` : ""}
        </p>
      )}
      {item.observation && <EvidenceOriginNote origin={item.observation.origin} hasBasis={item.observation.scanFinishedAt !== null} />}
      {item.openable ? (
        <p className="actions">
          <button type="button" className="secondary" onClick={() => onOpen({ alias: item.alias, platform: item.platform, surface: item.surface, entryId: item.entryId })}>
            Matriste incele{" "}<span className="sr-only">({item.entryName} · {item.entryId}; {where})</span>
          </button>
        </p>
      ) : (
        <p className="small bad-text" role="note">
          ⚠ Bu hedef ({platformLabel(item.platform)} · {surfaceLabel(item.surface)}) bu sürümde matriste açılamıyor; başka bir hedef seçilmedi.
        </p>
      )}
    </li>
  );
}

function groupByProject(items: ReviewItem[]): { alias: string; name: string; synthetic: boolean; items: ReviewItem[] }[] {
  const groups: { alias: string; name: string; synthetic: boolean; items: ReviewItem[] }[] = [];
  for (const i of items) {
    const last = groups[groups.length - 1];
    if (last && last.alias === i.alias) last.items.push(i);
    else groups.push({ alias: i.alias, name: i.projectName, synthetic: i.synthetic, items: [i] }); // items arrive sorted by project, so a group never reappears
  }
  return groups;
}

export function ReviewQueue({ projects, inventory, onOpen }: {
  projects: LoadState<ProjectView[]>;
  inventory: LoadState<Inventory>;
  onOpen: (request: TargetRequest & { entryId: string }) => void;
}) {
  const uid = useId();
  const [project, setProject] = useState<string>("all");
  const [reason, setReason] = useState<ReasonFilter>("all");
  // the clock is read once per answer (not on a timer): a reminder that falls due while the page stays open shows up at the next read
  const now = useMemo(() => Date.now(), [projects.data, inventory.data]); // eslint-disable-line react-hooks/exhaustive-deps
  const queue = useMemo(() => deriveReview(projects, inventory, now), [projects, inventory, now]);

  const projectOptions = useMemo(() => {
    const seen = new Map<string, string>();
    for (const i of queue.items) seen.set(i.alias, i.projectName);
    return [...seen.entries()];
  }, [queue.items]);
  const effectiveProject = project === "all" || projectOptions.some(([a]) => a === project) ? project : "all";
  const shown = useMemo(() => filterItems(queue.items, effectiveProject, reason), [queue.items, effectiveProject, reason]);
  const groups = useMemo(() => groupByProject(shown), [shown]);
  // uncertain-evidence decisions have no reason, so only the project filter can apply to them; with a reason filter on they are not listed - and that is said
  const uncertainShown = useMemo(() => (reason === "all" ? queue.uncertain.filter((i) => effectiveProject === "all" || i.alias === effectiveProject) : []), [queue.uncertain, effectiveProject, reason]);

  const unreliable = queue.projects === "failed" || queue.projects === "old" || queue.evidence === "failed" || queue.evidence === "old";
  const reread = () => { projects.reload(); inventory.reload(); };

  const summary = reviewHeadline(queue);

  return (
    <section className="panel" aria-labelledby={`${uid}-h`}>
      <h2 id={`${uid}-h`}>İncelenmesi Gerekenler</h2>
      <p className="muted small">
        Bu liste yalnızca <strong>kayıtlı kararlarınızdan</strong>, katalog işaretlerinden, hatırlatma tarihlerinden ve son <strong>kayıtlı</strong> envanter sonucundan türetilir;
        hiçbir şey kaydetmez, taramaz, kurmaz veya değiştirmez. Bir öğeyi görmek veya açmak sayıyı azaltmaz: sayı yalnızca kararınız, katalog durumu, tarih ya da kayıtlı gözlem gerçekten değiştiğinde değişir.
        İstek, kayıtlı gözlem, katalog uyumluluğu ve kurulum kaydı ayrı şeylerdir; kayıtlı gözlem canlı bir denetim değildir, ad eşleşmesi olabilir ve bilgisayar genelindeki bir aramadan gelebilir.
        Kararlar yalnızca aşağıdaki “Proje Araç Matrisi”nde verilir veya sıfırlanır.
      </p>

      <div role="status" className="small">
        <p>{summary}</p>
        {queue.projects !== "loading" && queue.projects !== "failed" && queue.evidence === "loading" && (
          <p className="bad-text">⚠ Kayıtlı envanter okunuyor; gözlemden doğan nedenler henüz dahil değil.</p>
        )}
        {queue.projects !== "loading" && queue.projects !== "failed" && queue.evidence === "failed" && (
          <p className="bad-text">⚠ Kayıtlı envanter okunamadı; gözlemden doğan nedenler dahil değil ({queue.evidenceUnavailableFor} kabul edilmiş istek için kayıtlı gözlem bilinmiyor). Bu, “taranmadı” veya “bulunamadı” anlamına gelmez.</p>
        )}
        {queue.projects === "refreshing" && <p className="bad-text">⚠ Kararlar yeniden okunuyor; gösterilenler kesin değil, okuma bitince değişebilir.</p>}
        {queue.evidence === "refreshing" && <p className="bad-text">⚠ Kayıtlı envanter yeniden okunuyor; gözlemden doğan nedenler kesin değil.</p>}
        {queue.projects === "old" && <p className="bad-text">⚠ Kararlar yenilenemedi; gösterilenler önceki başarılı okumadan ve eski olabilir.</p>}
        {queue.evidence === "old" && <p className="bad-text">⚠ Kayıtlı envanter yenilenemedi; gözlemden doğan nedenler önceki başarılı okumadan ve eski olabilir.</p>}
      </div>
      {unreliable && (
        <p><button type="button" className="secondary" onClick={reread}>Listeyi yeniden oku</button></p>
      )}

      {queue.items.length > 0 && (
        <div role="search" aria-label="İncelenecek öğeleri süz" className="row">
          <div className="field">
            <label htmlFor={`${uid}-p`}>Kuyrukta proje süz</label>
            <select id={`${uid}-p`} value={effectiveProject} onChange={(e) => setProject(e.target.value)}>
              <option value="all">Tümü</option>
              {projectOptions.map(([alias, name]) => <option key={alias} value={alias}>{name}</option>)}
            </select>
          </div>
          <div className="field">
            <label htmlFor={`${uid}-r`}>Kuyrukta neden süz</label>
            <select id={`${uid}-r`} value={reason} onChange={(e) => setReason(e.target.value as ReasonFilter)}>
              {REASON_FILTERS.map((f) => <option key={f.value} value={f.value}>{f.label}</option>)}
            </select>
          </div>
        </div>
      )}
      {queue.items.length > 0 && (
        <p className="small" role="status">{shown.length} / {queue.items.length} öğe gösteriliyor.</p>
      )}

      {groups.map((g) => (
        <div key={g.alias}>
          <h3>{g.name} {g.synthetic && <span className="tag">örnek veri</span>} <span className="muted small">({g.items.length})</span></h3>
          <ul className="rq-list">{g.items.map((i) => <Item key={i.key} item={i} onOpen={onOpen} />)}</ul>
        </div>
      ))}

      {queue.uncertain.length > 0 && reason !== "all" && (
        <p className="small bad-text" role="status">⚠ Neden süzgeci açıkken kanıtı belirsiz kalan {queue.uncertain.length} karar listelenmez; süzgeci “Tümü” yapın.</p>
      )}
      {uncertainShown.length > 0 && (
        <div>
          <h3>Kanıtı belirsiz kalan kararlar <span className="muted small">({uncertainShown.length})</span></h3>
          <p className="small muted">Bunlar için bir inceleme nedeni bulunamadı, ancak elimizdeki kayıt eksik veya çelişkili; “sorun var” ya da “sorun yok” denmiyor.</p>
          <ul className="rq-list">{uncertainShown.map((i) => <Item key={i.key} item={i} onOpen={onOpen} />)}</ul>
        </div>
      )}
    </section>
  );
}
