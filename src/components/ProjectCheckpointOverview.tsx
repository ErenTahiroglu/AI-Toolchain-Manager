// ATM-GOV-02 P2C-WP2 - the card view of the project checkpoint overview. Presentation only: every fact comes from `OverviewRow` (src/projectCheckpoint.ts), and
// nothing here calls the native side, the network or any storage. Each fact says where it comes from: [ECE] (a machine observation made in this session),
// [beyan] (typed by the user, unverified) or [bilinmiyor] (no source). A claim is never styled as, or turned into, an ECE result.
import { useId, useState, type FormEvent } from "react";
import { CI_LABEL, ERROR_LABEL, LABEL_AUTHORIZATION, LABEL_HISTORICAL, LABEL_MACHINE, LABEL_REVIEW, OUTCOME_LABEL, PROVIDER_FAILURE_LABEL } from "../ciAssessment";
import { CLAIM_LIMITS, formFromClaims, parseClaimsForm, type ClaimsFormInput, type ClaimsFormField } from "../claimsForm";
import type { AtmRow, OverviewRow, RowNote, UserClaims } from "../projectCheckpoint";

export type Source = "ECE" | "beyan" | "bilinmiyor";
export const SourceTag = ({ s }: { s: Source }) => <span className={`ov-src ov-src-${s}`}>[{s}]</span>;

export const RECOMMENDED_TEXT: Record<string, string> = {
  ENTER_ACCEPTED_CHECKPOINT: "Beyan edilen son kabul commit’ini girin.",
  RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA: "Beyan edilen kabul commit’i için aşağıdaki ECE denetim formunu siz çalıştırın (çalıştırma kimliğini ve denemeyi elle girersiniz).",
  RETRY_LATER: "GitHub’dan kanıt alınamadı; bir süre sonra yeniden deneyin. Bu bir CI başarısızlığı değildir.",
  REVIEW_ASSESSMENT_ERROR: "Değerlendirme tamamlanmadı; girdileri ve hata nedenini inceleyin. Bu bir CI başarısızlığı değildir.",
  FIX_OR_RERUN: "Zorunlu iş(ler) başarısız ya da atlandı; düzeltin ya da yeniden çalıştırın.",
  RESOLVE_BLOCKERS: "Beyan edilen açık engelleri çözün.",
  OUTSIDE_ATM_STEPS_REMAIN: "ATM içinde önerilecek başka bir adım yok. Bu bir kabul ya da tamamlanma değildir; kalan eksikler aşağıdaki “devam eden eksikler” bölümündedir.",
  ENTER_CLAIMS: "Bu proje için beyanları girin.",
  EXTERNAL_VERIFICATION_REQUIRED: "Bu depo için ATM içinde ECE doğrulaması desteklenmiyor; doğrulamayı ATM dışında yapın.",
};
const SHORT_ACTION: Record<string, string> = {
  ENTER_ACCEPTED_CHECKPOINT: "Kabul commit’i girin", RUN_ECE_FOR_CLAIMED_ACCEPTED_SHA: "ECE denetimi çalıştırın", RETRY_LATER: "Sonra yeniden deneyin",
  REVIEW_ASSESSMENT_ERROR: "Değerlendirme hatasını inceleyin", FIX_OR_RERUN: "Düzeltin / yeniden çalıştırın", RESOLVE_BLOCKERS: "Engelleri çözün",
  OUTSIDE_ATM_STEPS_REMAIN: "ATM dışı adımlar kalıyor", ENTER_CLAIMS: "Beyanları girin", EXTERNAL_VERIFICATION_REQUIRED: "ATM dışında doğrulayın",
};
const NOTE_TEXT: Record<RowNote, string> = {
  DUPLICATE_ATM_REPOSITORY_CLAIM: "Yinelenen depo beyanı: ATM deposunu beyan eden daha eski bir proje var; bu satır ECE gözlemi almaz.",
  VIRTUAL_ATM_ROW_NO_CLAIMS: "Bu sabit ATM satırıdır; ATM’yi bir proje olarak kaydedip depo beyanını girmediğiniz için beyan alanları yoktur.",
  OBSERVED_COMMIT_DIFFERS_FROM_CLAIMED_ACCEPTED: "Gözlem, beyan edilen son kabul commit’ine bağlı değil: ECE başka bir commit’i gözlemledi.",
  CLAIMED_HEAD_DIFFERS_FROM_OBSERVED_COMMIT: "Beyan edilen HEAD, gözlemlenen commit’ten farklı (yalnız bir beyan karşılaştırması; HEAD doğrulanmadı).",
};

const claimsOf = (r: OverviewRow): UserClaims | null => r.claims;
const rowKey = (r: OverviewRow): string => (r.identity.kind === "reference" ? r.identity.projectId : r.identity.rowKey);
const rowName = (r: OverviewRow): string => (r.identity.kind === "reference" ? r.identity.displayName : "ATM (sabit hedef)");

function ObservationBlock({ row }: { row: AtmRow }) {
  const ci = row.observed.ci;
  if (ci === null) {
    return <p className="small">Bu oturumda ECE makine gözlemi yok <SourceTag s="bilinmiyor" />. Aşağıdaki ECE formunu çalıştırdığınızda burada görünür.</p>;
  }
  const v = ci.view;
  return (
    <div>
      <p className="small"><strong>{LABEL_MACHINE}</strong> <SourceTag s="ECE" /></p>
      <dl className="facts small">
        <dt>Gözlenen commit</dt><dd><code>{ci.asked.commitSha}</code> <SourceTag s="ECE" /></dd>
        <dt>Çalıştırma · deneme</dt><dd>{ci.asked.runId} · {ci.asked.runAttempt} <SourceTag s="ECE" /></dd>
        <dt>Makine CI sonucu</dt><dd>{CI_LABEL[v.ci]} <SourceTag s="ECE" /></dd>
        <dt>ECE genel sonucu</dt><dd>{OUTCOME_LABEL[v.outcome]}</dd>
      </dl>
      {v.error !== null && (
        <p className="small ov-warn">
          {ERROR_LABEL[v.error]}.{v.providerFailure !== null && <> {PROVIDER_FAILURE_LABEL[v.providerFailure]}.</>} Bu, gözlenmiş bir CI başarısızlığı değildir; CI sonucu hakkında bir şey söylenmedi.
        </p>
      )}
      <p className="small muted">{LABEL_HISTORICAL}</p>
    </div>
  );
}

function ClaimsEditor({ row, onSave, onClear }: { row: OverviewRow; onSave: (projectId: string, claims: UserClaims) => string | null; onClear: (projectId: string) => void }) {
  const uid = useId();
  const id = row.identity.kind === "reference" ? row.identity.projectId : null;
  const [form, setForm] = useState<ClaimsFormInput>(() => formFromClaims(row.claims));
  const [errors, setErrors] = useState<Partial<Record<ClaimsFormField, string>>>({});
  const [saved, setSaved] = useState(false);
  if (id === null) return null;
  const set = (patch: Partial<ClaimsFormInput>) => { setForm({ ...form, ...patch }); setSaved(false); };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const parsed = parseClaimsForm(form);
    if (!parsed.ok) { setErrors(parsed.errors); setSaved(false); return; }
    const conflict = onSave(id, parsed.claims);
    if (conflict !== null) { setErrors({ repository: conflict }); setSaved(false); return; }
    setErrors({});
    setSaved(true);
  };
  const clear = () => { onClear(id); setForm(formFromClaims(null)); setErrors({}); setSaved(false); };
  const err = (f: ClaimsFormField) => errors[f];
  return (
    <details className="ov-edit">
      <summary>Beyanları düzenle (yalnız bu oturum)</summary>
      <p className="small ov-warn">Kalıcı değil: beyanlar yalnız bu sayfanın belleğinde durur; sayfadan ayrıldığınızda ya da uygulamayı kapattığınızda kaybolur. Yazdıklarınız birer beyandır; hiçbiri doğrulanmaz ve hiçbiri ECE sonucu olmaz.</p>
      <form onSubmit={submit} noValidate>
        <div className="field">
          <label htmlFor={`${uid}-repo`}>Depo (sahip/ad) — beyan</label>
          <input id={`${uid}-repo`} type="text" autoComplete="off" spellCheck={false} value={form.repository} aria-invalid={err("repository") !== undefined}
            aria-describedby={`${uid}-e-repo`} onChange={(e) => set({ repository: e.target.value })} />
          <span id={`${uid}-e-repo`} className="small bad-text" role={err("repository") ? "alert" : undefined}>{err("repository")}</span>
        </div>
        <div className="field">
          <label htmlFor={`${uid}-acc`}>Son kabul olduğu beyan edilen commit (40 küçük harfli onaltılık) — beyan</label>
          <input id={`${uid}-acc`} type="text" autoComplete="off" spellCheck={false} value={form.acceptedSha} aria-invalid={err("acceptedSha") !== undefined}
            aria-describedby={`${uid}-e-acc`} onChange={(e) => set({ acceptedSha: e.target.value })} />
          <span id={`${uid}-e-acc`} className="small bad-text" role={err("acceptedSha") ? "alert" : undefined}>{err("acceptedSha")}</span>
        </div>
        <div className="field">
          <label htmlFor={`${uid}-head`}>Güncel dalın ucu olduğu beyan edilen commit (HEAD, 40 küçük harfli onaltılık) — beyan</label>
          <input id={`${uid}-head`} type="text" autoComplete="off" spellCheck={false} value={form.claimedHeadSha} aria-invalid={err("claimedHeadSha") !== undefined}
            aria-describedby={`${uid}-e-head`} onChange={(e) => set({ claimedHeadSha: e.target.value })} />
          <span id={`${uid}-e-head`} className="small bad-text" role={err("claimedHeadSha") ? "alert" : undefined}>{err("claimedHeadSha")}</span>
        </div>
        <div className="choice"><input id={`${uid}-rev`} type="checkbox" checked={form.reviewClaim} onChange={(e) => set({ reviewClaim: e.target.checked })} />
          <label htmlFor={`${uid}-rev`}>Bağımsız inceleme yapıldığını beyan ediyorum (doğrulanmaz)</label></div>
        <div className="choice"><input id={`${uid}-auth`} type="checkbox" checked={form.authorizationClaim} onChange={(e) => set({ authorizationClaim: e.target.checked })} />
          <label htmlFor={`${uid}-auth`}>İnsan onayı verildiğini beyan ediyorum (doğrulanmaz)</label></div>
        <div className="field">
          <label htmlFor={`${uid}-blk`}>Açık engeller — beyan (satır başına bir engel; isteğe bağlı sonuna “ #123”)</label>
          <textarea id={`${uid}-blk`} rows={3} spellCheck={false} value={form.blockers} aria-invalid={err("blockers") !== undefined}
            aria-describedby={`${uid}-e-blk`} onChange={(e) => set({ blockers: e.target.value })} />
          <span id={`${uid}-e-blk`} className="small bad-text" role={err("blockers") ? "alert" : undefined}>{err("blockers")}</span>
          <span className="small muted">En çok {CLAIM_LIMITS.maxBlockers} engel, her biri en çok {CLAIM_LIMITS.maxBlockerText} karakter. Issue numarası yalnız bir metindir; durumu sorgulanmaz.</span>
        </div>
        <p className="actions">
          <button type="submit">Oturuma uygula</button>
          <button type="button" className="secondary" onClick={clear}>Bu projenin beyanlarını temizle</button>
        </p>
        <p className="small" aria-live="polite">{saved ? "Beyanlar bu oturum için uygulandı (kalıcı değil)." : ""}</p>
      </form>
    </details>
  );
}

function Card({ row, onSave, onClear, onGoToPanel }: { row: OverviewRow; onSave: (id: string, c: UserClaims) => string | null; onClear: (id: string) => void; onGoToPanel: () => void }) {
  const uid = useId();
  const c = claimsOf(row);
  const isAtm = row.role === "ATM";
  const none = (s: Source = "bilinmiyor") => <>girilmemiş <SourceTag s={s} /></>;
  const obs = row.observed;
  return (
    <div className="ov-card" role="group" aria-labelledby={`${uid}-h`} data-role={row.role}>
      <header>
        <h3 id={`${uid}-h`}>{rowName(row)}</h3>
        <p className="ov-tags">
          <span className="tag">{isAtm ? "ATM · ECE denetimi var" : "ATM dışı"}</span>
          {row.identity.kind === "atm-fixed-target" && <span className="tag">sabit hedef</span>}
        </p>
        <p className="small">Depo: {c?.repository ? <><code>{c.repository}</code> <SourceTag s="beyan" /></> : none()}</p>
      </header>

      <section aria-label="Önerilen eylem" className="ov-action">
        <p><strong>{SHORT_ACTION[row.recommended.kind]}</strong></p>
        <p className="small">{RECOMMENDED_TEXT[row.recommended.kind]}</p>
        {isAtm && <button type="button" className="secondary" onClick={onGoToPanel}>ECE denetim formuna git</button>}
      </section>

      <section aria-label="Beyan edilen commit’ler">
        <h4>Commit’ler</h4>
        <dl className="facts small">
          <dt>Beyan edilen son kabul</dt><dd>{c?.acceptedSha ? <><code>{c.acceptedSha}</code> <SourceTag s="beyan" /></> : none()}</dd>
          <dt>Beyan edilen HEAD</dt><dd>{c?.claimedHeadSha ? <><code>{c.claimedHeadSha}</code> <SourceTag s="beyan" /></> : none()}</dd>
          <dt>Güncel HEAD</dt><dd>doğrulanmadı — bilinmiyor <SourceTag s="bilinmiyor" /></dd>
        </dl>
      </section>

      {isAtm && <section aria-label="ECE makine gözlemi"><h4>ECE makine gözlemi</h4><ObservationBlock row={row as AtmRow} /></section>}

      <section aria-label="İnceleme ve onay">
        <h4>İnceleme ve onay</h4>
        <dl className="facts small">
          <dt>Gözlenen inceleme</dt>
          <dd>{obs.review === "UNREVIEWED" ? <>{LABEL_REVIEW} <SourceTag s="ECE" /></> : <>değerlendirilmedi <SourceTag s="bilinmiyor" /></>}</dd>
          <dt>Gözlenen onay</dt>
          <dd>{obs.authorization === "AUTHORIZATION_PENDING" ? <>{LABEL_AUTHORIZATION} <SourceTag s="ECE" /></> : <>değerlendirilmedi <SourceTag s="bilinmiyor" /></>}</dd>
          <dt>Beyan edilen inceleme</dt>
          <dd>{c?.reviewClaim === "CLAIMED_INDEPENDENT" ? <>bağımsız inceleme yapıldığı beyan edildi — doğrulanmadı <SourceTag s="beyan" /></> : none()}</dd>
          <dt>Beyan edilen onay</dt>
          <dd>{c?.authorizationClaim === "CLAIMED_GRANTED" ? <>insan onayı verildiği beyan edildi — doğrulanmadı <SourceTag s="beyan" /></> : none()}</dd>
        </dl>
      </section>

      <section aria-label="Açık engeller">
        <h4>Açık engeller <SourceTag s="beyan" /></h4>
        {c && c.blockers.length > 0 ? (
          <ul className="small">{c.blockers.map((b, i) => <li key={i}>{b.text}{b.issue !== null && <> <span className="muted">(#{b.issue}; yalnız numara, durumu sorgulanmadı)</span></>}</li>)}</ul>
        ) : <p className="small">Beyan edilmiş engel yok <SourceTag s="bilinmiyor" /></p>}
      </section>

      <section aria-label="Devam eden eksikler" className="ov-standing">
        <h4>Devam eden eksikler (bu sürümde kaldırılamaz)</h4>
        <ul className="small">
          <li>HEAD doğrulanmadı</li>
          <li>Bağımsız inceleme doğrulanmadı</li>
          <li>İnsan onayı doğrulanmadı</li>
        </ul>
      </section>

      {row.notes.length > 0 && <ul className="small muted" aria-label="Notlar">{row.notes.map((n) => <li key={n}>{NOTE_TEXT[n]}</li>)}</ul>}
      <ClaimsEditor key={rowKey(row)} row={row} onSave={onSave} onClear={onClear} />
    </div>
  );
}

type RoleFilter = "all" | "ATM" | "FOREIGN";

export function ProjectCheckpointOverview({ rows, hiddenSynthetic, onSave, onClear, onGoToPanel }: {
  rows: OverviewRow[]; hiddenSynthetic: number; onSave: (id: string, c: UserClaims) => string | null; onClear: (id: string) => void; onGoToPanel: () => void;
}) {
  const uid = useId();
  const [query, setQuery] = useState("");
  const [role, setRole] = useState<RoleFilter>("all");
  const [action, setAction] = useState("all");
  const q = query.trim().toLowerCase();
  const shown = rows.filter((r) =>
    (role === "all" || r.role === role) && (action === "all" || r.recommended.kind === action) &&
    (q === "" || rowName(r).toLowerCase().includes(q) || (r.claims?.repository ?? "").toLowerCase().includes(q)));
  const kinds = [...new Set(rows.map((r) => r.recommended.kind))];
  return (
    <section className="ov" aria-labelledby={`${uid}-h`}>
      <h2 id={`${uid}-h`}>Proje Checkpoint Özeti</h2>
      <p className="ov-lead">
        Kayıtlı gerçek projelerinizin checkpoint durumu tek yerde. Her bilginin kaynağı yazılıdır: <SourceTag s="ECE" /> bu oturumda yaptığınız makine denetimi,{" "}
        <SourceTag s="beyan" /> sizin yazdığınız doğrulanmamış değer, <SourceTag s="bilinmiyor" /> kaynak yok. Bir beyan hiçbir zaman ECE sonucu olmaz ve geçmiş bir CI başarısı güncel HEAD onayı değildir.
      </p>
      <p className="ov-lead small ov-warn">Beyanlar kalıcı değildir; yalnız bu oturumda, bu sayfanın belleğinde tutulur.</p>
      {hiddenSynthetic > 0 && <p className="small muted">{hiddenSynthetic} örnek (sentetik) proje gerçek depo olmadığı için gösterilmiyor.</p>}

      <form className="ov-filters" role="search" aria-label="Kartları süz" onSubmit={(e) => e.preventDefault()}>
        <div className="field"><label htmlFor={`${uid}-q`}>Ara (ad ya da depo)</label>
          <input id={`${uid}-q`} type="text" autoComplete="off" value={query} onChange={(e) => setQuery(e.target.value)} /></div>
        <div className="field"><label htmlFor={`${uid}-r`}>Rol</label>
          <select id={`${uid}-r`} value={role} onChange={(e) => setRole(e.target.value as RoleFilter)}>
            <option value="all">Tümü</option><option value="ATM">ATM (ECE denetimi var)</option><option value="FOREIGN">ATM dışı</option>
          </select></div>
        <div className="field"><label htmlFor={`${uid}-a`}>Önerilen eylem</label>
          <select id={`${uid}-a`} value={action} onChange={(e) => setAction(e.target.value)}>
            <option value="all">Tümü</option>
            {kinds.map((k) => <option key={k} value={k}>{SHORT_ACTION[k]}</option>)}
          </select></div>
      </form>
      <p className="small muted" aria-live="polite">{shown.length} / {rows.length} kart gösteriliyor. Süzme yalnız bu sayfadaki verilere uygulanır; hiçbir istek göndermez.</p>

      {shown.length === 0 ? <p className="small">Süzgece uyan kart yok.</p> : (
        <div className="ov-grid">
          {shown.map((r) => <Card key={rowKey(r)} row={r} onSave={onSave} onClear={onClear} onGoToPanel={onGoToPanel} />)}
        </div>
      )}

      <details className="ov-table">
        <summary>Ayrıntı tablosu</summary>
        <div className="table-wrap">
          <table>
            <caption className="sr-only">Projelerin checkpoint ayrıntıları</caption>
            <thead><tr><th scope="col">Proje</th><th scope="col">Rol</th><th scope="col">Depo [beyan]</th><th scope="col">Kabul commit’i [beyan]</th><th scope="col">HEAD [beyan]</th><th scope="col">ECE makine CI [ECE]</th><th scope="col">Önerilen eylem</th></tr></thead>
            <tbody>
              {shown.map((r) => (
                <tr key={rowKey(r)}>
                  <th scope="row">{rowName(r)}</th>
                  <td>{r.role === "ATM" ? "ATM" : "ATM dışı"}</td>
                  <td>{r.claims?.repository ?? "girilmemiş [bilinmiyor]"}</td>
                  <td>{r.claims?.acceptedSha ? <code>{r.claims.acceptedSha}</code> : "girilmemiş [bilinmiyor]"}</td>
                  <td>{r.claims?.claimedHeadSha ? <code>{r.claims.claimedHeadSha}</code> : "girilmemiş [bilinmiyor]"}</td>
                  <td>{r.observed.ci ? CI_LABEL[r.observed.ci.view.ci] : r.role === "ATM" ? "gözlem yok [bilinmiyor]" : "bu proje için desteklenmiyor"}</td>
                  <td>{SHORT_ACTION[r.recommended.kind]}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </details>
    </section>
  );
}
