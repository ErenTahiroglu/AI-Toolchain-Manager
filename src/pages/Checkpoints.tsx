// ATM-GOV-01 - "Checkpoint Merkezi": a read-only viewer for checkpoint metadata the user pastes by hand. It is NOT a verifier: whatever the text claims
// stays a claim and is labelled "KAYNAK BEYANI — DOĞRULANMADI". The text and the parsed view live only in this component's state; the app re-mounts a page
// on every navigation, so leaving the page destroys both. No IPC, no network, no file, no clipboard, no storage, no timer, no link, no HTML rendering.
// All logic and all wording are in src/checkpointView.ts; contract: docs/atm-gov-01/CONTRACT_AND_BOUNDARY.md.
import { useEffect, useId, useRef, useState } from "react";
import {
  ASSESSMENT_LABEL, AVAILABILITY_LABEL, CONCLUSION_LABEL, ERROR_TEXT, LIMITS, MISSING_LABEL, NOT_CHECKED_SENTENCE, NOTE_TEXT, parseCheckpointInput, ROLE_LABEL, SCHEMA_VERSION,
  SCOPE_SENTENCE, TRUST_LABEL, type CheckpointView, type ErrorCode, type NoteCode,
} from "../checkpointView";

type View =
  | { kind: "idle" }
  | { kind: "error"; code: ErrorCode; path: string }
  | { kind: "shown"; checkpoints: CheckpointView[]; shownAt: number };

/** A made-up record that only shows the shape. Static text: it is never parsed, stored or offered for copying. */
const EXAMPLE = `{
  "schemaVersion": "${SCHEMA_VERSION}",
  "checkpoints": [
    {
      "repository": "ornek-sahip/ornek-depo",
      "phase": "ORNEK-01",
      "sourceSha": "0123456789abcdef0123456789abcdef01234567",
      "workflows": [
        { "name": "CI", "runId": 1234567, "runAttempt": 1,
          "jobs": [ { "name": "windows-x64", "claimedConclusion": "success" } ] }
      ],
      "issues": [1],
      "limitations": ["Yalnizca beyan"]
    }
  ]
}`;

const Notes = ({ notes }: { notes: NoteCode[] }) =>
  notes.length === 0 ? null : (
    <ul className="small muted" aria-label="Notlar">
      {notes.map((n) => <li key={n}>{NOTE_TEXT[n]}</li>)}
    </ul>
  );

function CheckpointCard({ c, index, shownAt }: { c: CheckpointView; index: number; shownAt: number }) {
  const uid = useId();
  return (
    <article className="panel cp-card" aria-labelledby={`${uid}-h`}>
      <h2 id={`${uid}-h`}>
        Kayıt {index + 1}: {c.repository} <span className="muted">· {c.phase}</span>
      </h2>
      <p className="banner banner-claim small" role="note">
        <strong>{c.trust}</strong> <span>{NOT_CHECKED_SENTENCE}</span>
      </p>
      <dl className="facts">
        <dt>Depo (bildirilen)</dt><dd>{c.repository}</dd>
        <dt>Aşama (bildirilen)</dt><dd>{c.phase}</dd>
        <dt>Kaynak commit (bildirilen, tam)</dt><dd><code>{c.sourceSha}</code></dd>
        <dt>Kapsam</dt><dd>{SCOPE_SENTENCE}</dd>
      </dl>
      <Notes notes={c.notes} />

      {c.workflows.map((w, i) => (
        <section key={`${w.runId}/${w.runAttempt}`} aria-labelledby={`${uid}-w${i}`}>
          <h3 id={`${uid}-w${i}`}>İş akışı beyanı: {w.name}</h3>
          <dl className="facts">
            <dt>Çalıştırma kimliği (bildirilen)</dt><dd>{w.runId}</dd>
            <dt>Deneme (bildirilen)</dt><dd>{w.runAttempt}</dd>
            {w.ref !== null && (<><dt>Tetikleyen ref (yalnızca beyan)</dt><dd>{w.ref}</dd></>)}
          </dl>
          {w.jobs.length > 0 && (
            <ul aria-label={`${w.name}: bildirilen işler`}>
              {w.jobs.map((j) => (
                <li key={j.name}>{j.name} — bildirilen sonuç: <strong>{CONCLUSION_LABEL[j.claimed]}</strong> <span className="muted">(denetlenmedi)</span></li>
              ))}
            </ul>
          )}
          {w.expectedJobs !== null && (
            <p className="small">Beklenen işler (bildirilen): {w.expectedJobs.length === 0 ? "liste boş" : w.expectedJobs.join(", ")}.</p>
          )}
          <Notes notes={w.notes} />
        </section>
      ))}

      {c.artifacts.length > 0 && (
        <section aria-labelledby={`${uid}-a`}>
          <h3 id={`${uid}-a`}>Çıktı beyanları</h3>
          <p className="small muted">
            Son kullanma zamanı, yalnızca bildirilen zaman damgasının “Göster”e bastığınız andaki bu cihaz saatiyle ({new Date(shownAt).toISOString()}) karşılaştırılmasıdır; çıktının
            şimdi var olup olmadığı denetlenmedi.
          </p>
          <ul>
            {c.artifacts.map((a) => (
              <li key={a.id}>
                <strong>Çıktı kimliği {a.id}</strong>{a.name !== null ? ` · ${a.name}` : ""}
                <dl className="facts small">
                  <dt>SHA-256 (bildirilen)</dt><dd>{a.sha256 !== null ? <code>{a.sha256}</code> : "bildirilmemiş"}</dd>
                  <dt>Son kullanma (bildirilen)</dt>
                  <dd>
                    {a.expiresAt !== null ? <code>{a.expiresAt}</code> : "bildirilmemiş"}
                    {a.expiry === "declared_past" && <span> — bildirilen zaman, bu cihazın saatine göre geçmiş görünüyor</span>}
                    {a.expiry === "declared_future" && <span> — bildirilen zaman, bu cihazın saatine göre henüz gelmemiş görünüyor</span>}
                  </dd>
                  <dt>Durum (bildirilen)</dt><dd>{a.availabilityClaim !== null ? AVAILABILITY_LABEL[a.availabilityClaim] : "bildirilmemiş"}</dd>
                </dl>
                <Notes notes={a.notes} />
              </li>
            ))}
          </ul>
        </section>
      )}

      {(c.issues.length > 0 || c.pullRequests.length > 0) && (
        <section aria-labelledby={`${uid}-r`}>
          <h3 id={`${uid}-r`}>İlgili kayıt numaraları</h3>
          <p className="small">
            {c.issues.length > 0 && <>Issue: {c.issues.map((n) => `#${n}`).join(", ")}. </>}
            {c.pullRequests.length > 0 && <>Çekme isteği: {c.pullRequests.map((n) => `#${n}`).join(", ")}. </>}
            <span className="muted">Yalnızca metin; bağlantı değildir, hiçbir şey açılmaz ya da getirilmez.</span>
          </p>
        </section>
      )}

      {c.reviewerClaim !== null && (
        <section aria-labelledby={`${uid}-v`}>
          <h3 id={`${uid}-v`}>İnceleyici beyanı</h3>
          <p>
            Kaynağın bildirdiği değerlendirme: <strong>{ASSESSMENT_LABEL[c.reviewerClaim.assessment]}</strong>
            {" "}({ROLE_LABEL[c.reviewerClaim.role]}).{" "}
            <span className="muted">Bu bir beyandır; bağımsız bir onay değildir ve doğrulanmadı.</span>
          </p>
          {c.reviewerClaim.onSha !== null && <p className="small">Değerlendirilen commit (bildirilen): <code>{c.reviewerClaim.onSha}</code></p>}
        </section>
      )}

      {(c.limitations.length > 0 || c.missingEvidence.length > 0) && (
        <section aria-labelledby={`${uid}-l`}>
          <h3 id={`${uid}-l`}>Sınırlar ve eksik kanıt (bildirilen)</h3>
          {c.limitations.length > 0 && <ul aria-label="Bildirilen sınırlar">{c.limitations.map((l, i) => <li key={i}>{l}</li>)}</ul>}
          {c.missingEvidence.length > 0 && <ul aria-label="Bildirilen eksik kanıt">{c.missingEvidence.map((m) => <li key={m}>{MISSING_LABEL[m]}</li>)}</ul>}
        </section>
      )}
    </article>
  );
}

export function CheckpointsPage() {
  const uid = useId();
  const [text, setText] = useState("");
  const [view, setView] = useState<View>({ kind: "idle" });
  const textRef = useRef<HTMLTextAreaElement>(null);
  const resultRef = useRef<HTMLElement>(null);

  // a successful "Göster" moves focus to the summary so keyboard and screen-reader users land on the result
  useEffect(() => {
    if (view.kind === "shown") resultRef.current?.focus();
  }, [view]);

  const edit = (value: string) => {
    setText(value);
    setView({ kind: "idle" }); // the screen never shows a result for text that is no longer in the box
  };

  const show = () => {
    const now = Date.now(); // read once, here: it only interprets DECLARED artifact expiry timestamps
    const r = parseCheckpointInput(text, now);
    if (r.ok) {
      setView({ kind: "shown", checkpoints: r.checkpoints, shownAt: now });
    } else {
      setView({ kind: "error", code: r.code, path: r.path }); // a failed attempt discards any earlier result
    }
  };

  const clear = () => {
    setText("");
    setView({ kind: "idle" });
    textRef.current?.focus();
  };

  return (
    <>
      <div className="banner banner-claim" role="note">
        <strong>{TRUST_LABEL}</strong>{" "}
        <span>
          Bu sayfa yalnızca elle yapıştırdığınız bir metni okunaklı gösterir. Ne bir doğrulayıcıdır ne de bir sertifika: metin “başarılı”, “kabul” ya da “doğrulanmış” dese de bu, kaynağın
          beyanı olarak kalır. Hiçbir şey ağdan, GitHub'dan ya da dosyadan okunmaz; yapıştırdığınız metin yalnızca bu sayfanın belleğinde durur, sayfadan ayrılınca silinir.
        </span>
      </div>
      <p className="muted">
        Üç kavram ayrı tutulur: <strong>kaynak beyanı</strong> (burada gösterilen), <strong>makine kanıtı</strong> (gerçek bir CI çalıştırması, çıktı ya da commit; bu sayfa bunlara bakmaz) ve{" "}
        <strong>bağımsız insan onayı</strong> (bir Red Team kararı; yapıştırılan bir alan bunun yerine geçemez). Ayrı bir doğrulayıcı henüz yoktur.
      </p>

      <section className="panel" aria-labelledby={`${uid}-in`}>
        <h2 id={`${uid}-in`}>Kayıt yapıştır</h2>
        <p className="small" id={`${uid}-help`}>
          Şema sürümü <code>{SCHEMA_VERSION}</code> olan, en çok {LIMITS.chars.toLocaleString("tr-TR")} karakterlik tek bir JSON metni yapıştırın. Bilinmeyen alanlar, tekrar eden alanlar, adres/işaretleme/gizli
          bilgi benzeri metinler ve sınırların dışındaki değerler reddedilir; hata iletisi yapıştırdığınız metni tekrar etmez.
        </p>
        <details>
          <summary>Beklenen biçim (uydurma örnek) ve sınırlar</summary>
          <pre className="cfg-pre" tabIndex={0} aria-label="Uydurma örnek kayıt">{EXAMPLE}</pre>
          <ul className="small">
            <li>En çok {LIMITS.checkpoints} kayıt; kayıt başına en çok {LIMITS.workflows} iş akışı, iş akışı başına en çok {LIMITS.jobs} iş, {LIMITS.artifacts} çıktı, {LIMITS.issues} issue ve {LIMITS.pullRequests} çekme isteği.</li>
            <li>Commit tam 40 küçük onaltılık karakter; SHA-256 tam 64. Çalıştırma kimliği ve deneme pozitif tam sayı.</li>
            <li>Serbest metin: harf, rakam, boşluk ve <code>. , : ; ( ) # + _ - ' ·</code> karakterleri; en çok {LIMITS.phase}–{LIMITS.limitation} karakter.</li>
            <li>Her alan ve her sayı yalnızca bir <em>sözdizimi</em> denetiminden geçer; var olduğuna dair kanıt değildir.</li>
          </ul>
        </details>
        <div className="field">
          <label htmlFor={`${uid}-t`}>Checkpoint metaverisi (JSON)</label>
          <textarea
            id={`${uid}-t`} ref={textRef} rows={12} autoComplete="off" spellCheck={false} value={text} aria-describedby={`${uid}-help ${uid}-st`}
            onChange={(e) => edit(e.target.value)}
          />
        </div>
        <p className="actions">
          <button type="button" onClick={show}>Göster<span className="sr-only"> (yalnızca okunaklı gösterir; hiçbir şeyi doğrulamaz, kaydetmez ya da göndermez)</span></button>
          <button type="button" className="secondary" onClick={clear}>Temizle<span className="sr-only"> (yapıştırılan metni ve gösterilen kaydı siler)</span></button>
        </p>
        <div id={`${uid}-st`} role="status" aria-live="polite">
          {view.kind === "idle" && text.trim() === "" && <p className="muted small">Henüz bir şey gösterilmiyor. Bir kayıt yapıştırıp “Göster”e basın.</p>}
        </div>
        {view.kind === "error" && (
          <div className="banner banner-error" role="alert">
            <strong>Kayıt gösterilemedi.</strong> {ERROR_TEXT[view.code]} <code>{view.code}</code>
            {view.path !== "$" && <> · alan: <code>{view.path}</code></>}
            <span className="muted"> Hiçbir şey gösterilmedi, kaydedilmedi ya da gönderilmedi.</span>
          </div>
        )}
      </section>

      {view.kind === "shown" && (
        <section ref={resultRef} tabIndex={-1} aria-label="Kayıt özeti" className="cp-result">
          <p className="small muted">
            {view.checkpoints.length} kayıt gösteriliyor. Hepsi <strong>{TRUST_LABEL}</strong>. Bir kaydın “başarılı” demesi bağımsız kabul anlamına gelmez; kayıtlar toplanıp bir sonuca çevrilmez.
          </p>
          {view.checkpoints.map((c, i) => <CheckpointCard key={`${c.repository}/${c.sourceSha}/${c.phase}`} c={c} index={i} shownAt={view.shownAt} />)}
        </section>
      )}
    </>
  );
}
