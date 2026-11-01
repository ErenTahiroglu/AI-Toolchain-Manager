// ATM-GOV-02 P2B - "GitHub CI Kanıtını Denetle (ECE)": ONE explicit, user-initiated, read-only look at the public CI evidence of ONE run of THIS
// repository, judged by the independent ECE verifier in Rust. It is rendered on the Checkpoint Merkezi page but is SEPARATE from the manual
// viewer above it: that viewer parses text the user pasted and keeps it as an UNTRUSTED claim; nothing pasted there is ever sent here.
//
// The only call to the native side happens inside the form's submit handler: never on mount, in an effect, on a timer, on navigation or at
// startup. The result lives in this component's state only (no storage, no cache). If the inputs are edited while a request is pending, that
// request's answer is discarded: an old answer is never shown for new inputs. The result is a machine CI observation: it is not an acceptance,
// not a review and not an authorization, and it is never styled as a success.
import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { api, toAppError } from "../api";
import {
  AUTHORIZATION_LABEL, AVAILABILITY_LABEL, BLOCKING_LABEL, CI_LABEL, describeReport, ERROR_LABEL, FRESHNESS_LABEL, INPUT_ERROR, LABEL_MACHINE, LIMITATION_LABEL,
  MAX_RUN_ATTEMPT, OUTCOME_LABEL, parseInput, PROVIDER_FAILURE_LABEL, REQUIRED_JOBS, REVIEW_LABEL, TARGET_REPOSITORY, TARGET_REPOSITORY_ID, TARGET_WORKFLOW,
  type AssessmentView, type InputField,
} from "../ciAssessment";
import type { AppError, AssessAtmCheckpointRequest } from "../types";

type View =
  | { kind: "idle" }
  | { kind: "invalid"; field: InputField }
  | { kind: "shown"; asked: AssessAtmCheckpointRequest; view: AssessmentView }
  | { kind: "error"; error: AppError };

function Result({ view }: { view: AssessmentView }) {
  if (!view.recognized) {
    return (
      <div className="banner banner-error" role="alert">
        <strong>Sonuç tanınmadı.</strong> Yerel çekirdekten gelen yanıt bu sayfanın bildiği kapalı değerlerle ya da sorulan hedefle uyuşmadı; hiçbir şey geçerli sayılmadı ve gösterilmedi.
      </div>
    );
  }
  const p = view.provenance;
  return (
    <>
      <p className="banner banner-note small" role="note">
        <strong>{LABEL_MACHINE}</strong>{" "}
        <span>Bu, geçmiş bir GitHub Actions çalıştırmasının makine gözlemidir. Bir checkpoint’in kabulü değildir; inceleme ya da insan onayı yerine geçmez.</span>
      </p>
      <dl className="facts">
        <dt>Hedef depo (sabit)</dt><dd>{p.repository} <span className="muted small">(ATM kodunda sabit; bu ad sizin girdiğiniz bir değer değildir)</span></dd>
        <dt>Sayısal depo kimliği (sabit)</dt><dd>{TARGET_REPOSITORY_ID}</dd>
        <dt>İş akışı (sabit)</dt><dd><code>{p.workflowPath}</code></dd>
        <dt>Tam commit</dt><dd><code>{p.commitSha}</code></dd>
        <dt>Çalıştırma kimliği · deneme</dt><dd>{p.runId} · {p.runAttempt}</dd>
        <dt>ECE değişmez revizyonu</dt><dd><code>{p.eceRevision}</code></dd>
        <dt>İlke</dt><dd>{p.policyId} · SHA-256 <code>{p.policyFileSha256}</code></dd>
      </dl>
      <h3>Değerlendirme</h3>
      <dl className="facts">
        <dt>Makine CI sonucu</dt><dd>{CI_LABEL[view.ci]} <code>{view.ci}</code></dd>
        <dt>ECE genel sonucu</dt><dd>{OUTCOME_LABEL[view.outcome]} <code>{view.outcome}</code></dd>
        <dt>Çıktı (artifact)</dt><dd>{AVAILABILITY_LABEL[view.availability]}</dd>
        <dt>İnceleme</dt><dd><strong>{REVIEW_LABEL[view.review]}</strong></dd>
        <dt>Onay</dt><dd><strong>{AUTHORIZATION_LABEL[view.authorization]}</strong></dd>
        <dt>Güncellik</dt><dd><strong>{FRESHNESS_LABEL[view.freshness]}</strong></dd>
      </dl>
      {view.error !== null && (
        <div className="banner banner-error" role="alert">
          <strong>{ERROR_LABEL[view.error]}.</strong>
          {view.providerFailure !== null && <> {PROVIDER_FAILURE_LABEL[view.providerFailure]}.</>}
          <span className="muted"> Bir ağ ya da sağlayıcı sorunu, gözlenmiş bir CI başarısızlığı değildir; bu denetimde CI sonucu hakkında bir şey söylenmedi.</span>
        </div>
      )}
      {view.blocking.length > 0 && (
        <section aria-label="Engelleyici nedenler">
          <h3>Engelleyici nedenler</h3>
          <ul>{view.blocking.map((b) => <li key={b}>{BLOCKING_LABEL[b]}</li>)}</ul>
        </section>
      )}
      <section aria-label="Sınırlar">
        <h3>Sınırlar</h3>
        <ul>{view.limitations.map((l) => <li key={l}>{LIMITATION_LABEL[l]}</li>)}</ul>
      </section>
    </>
  );
}

export function CiEvidencePanel() {
  const uid = useId();
  const [sha, setSha] = useState("");
  const [runId, setRunId] = useState("");
  const [attempt, setAttempt] = useState("1");
  const [view, setView] = useState<View>({ kind: "idle" });
  const [pending, setPending] = useState(false);
  const seq = useRef(0); // bumped by every edit and every submit: only the latest submit's answer may be shown
  const mounted = useRef(true);
  const resultRef = useRef<HTMLElement>(null);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false; // leaving the page: nothing is shown, stored or kept
    };
  }, []);
  useEffect(() => {
    if (view.kind === "shown" || view.kind === "error") resultRef.current?.focus();
  }, [view]);

  const edited = (set: (v: string) => void) => (value: string) => {
    set(value);
    seq.current += 1; // an answer that is still on its way belongs to the old inputs
    setView({ kind: "idle" }); // the screen never shows a result for inputs that are no longer in the boxes
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (pending) return;
    const parsed = parseInput(sha.trim(), runId.trim(), attempt.trim());
    if (!parsed.ok) {
      setView({ kind: "invalid", field: parsed.field }); // nothing is sent
      return;
    }
    const mine = ++seq.current;
    const asked = parsed.request;
    setPending(true);
    setView({ kind: "idle" });
    api.assessAtmCheckpoint(asked).then(
      (report) => {
        if (!mounted.current) return;
        setPending(false);
        if (mine !== seq.current) return; // the inputs changed meanwhile: this answer is not for them
        setView({ kind: "shown", asked, view: describeReport(report, asked) });
      },
      (e: unknown) => {
        if (!mounted.current) return;
        setPending(false);
        if (mine !== seq.current) return;
        setView({ kind: "error", error: toAppError(e) });
      },
    );
  };

  const invalidField = view.kind === "invalid" ? view.field : null;
  return (
    <section className="panel" aria-labelledby={`${uid}-h`}>
      <h2 id={`${uid}-h`}>GitHub CI Kanıtını Denetle (ECE)</h2>
      <div className="banner banner-note small" role="note">
        <strong>Yukarıdaki yapıştırma alanından ayrıdır.</strong> Orada yazılanlar kaynak beyanı olarak kalır ve buraya hiçbir şey aktarılmaz. Bu bölüm yalnızca sizin girdiğiniz commit,
        çalıştırma kimliği ve denemeyi, <strong>{TARGET_REPOSITORY}</strong> deposu (sayısal kimlik {TARGET_REPOSITORY_ID}), <code>{TARGET_WORKFLOW}</code> iş akışı ve zorunlu işler{" "}
        <code>{REQUIRED_JOBS[0]}</code> ile <code>{REQUIRED_JOBS[1]}</code> için, ECE doğrulayıcısına denetletir.
      </div>
      <ul className="small">
        <li>
          Düğmeye bastığınızda uygulamanın yerel (Rust) tarafı, <code>api.github.com</code> adresine <strong>tek seferlik, salt okunur, kimliksiz</strong> istekler gönderir.
          Doğrulama genellikle dört istek gerektirir (depo, commit, çalıştırma ve tek sayfalık iş listesi); iş listesi birden çok sayfaya yayılırsa ya da aynı adres içinde
          yönlendirme olursa daha fazla istek gönderilebilir. Bu uygulama kesin bir üst sınır uygulamaz. Yazma yapılmaz; hiçbir şey kaydedilmez.
        </li>
        <li>
          Bu çağrı yolu, ortamdaki GitHub belirteçlerini okumayan kimliksiz istemci oluşturucusunu kullanır ve bilerek bir GitHub <code>Authorization</code> başlığı eklemez.
          İşletim sistemi ya da ortam ara sunucu ayarları uygulanabilir; ara sunucu kimlik doğrulaması yapılandırılmışsa bunu HTTP altyapısı yönetir ve bu uygulama bunun olamayacağını iddia etmez.
        </li>
        <li>
          GitHub; IP adresinizi ve istemcinin sabit <code>User-Agent</code> değerini (<code>Evidence-Checkpoint-Engine-GOV-02</code>) görür. Kimliksiz erişim, saatte yaklaşık 60 istekle sınırlıdır.
          Bu yerel isteklerin dışında, gömülü WebView2 çalışma zamanının ayrı ağ trafiği (Issue #25) çözülmüş değildir ve bu ifadenin dışındadır.
        </li>
        <li>
          Bu sürüm yalnızca çalıştırmanın <strong>en son denemesini</strong> değerlendirir. Yeniden çalıştırılmış, yani yerini yeni bir denemeye bırakmış eski bir deneme reddedilir;
          iki denemenin verileri hiçbir zaman karıştırılmaz.
        </li>
        <li>Bir geçmiş çalıştırmanın sonucu, dalın şimdiki ucunu (HEAD) onaylamaz. Bir makine sonucu hiçbir zaman checkpoint kabulü, bağımsız inceleme ya da insan onayı sayılmaz.</li>
      </ul>
      <form onSubmit={submit} noValidate>
        <div className="field">
          <label htmlFor={`${uid}-sha`}>Tam commit SHA (40 küçük harfli onaltılık karakter)</label>
          <input id={`${uid}-sha`} type="text" autoComplete="off" spellCheck={false} value={sha} aria-invalid={invalidField === "commitSha"}
            aria-describedby={`${uid}-err`} onChange={(e) => edited(setSha)(e.target.value)} />
        </div>
        <div className="field">
          <label htmlFor={`${uid}-run`}>GitHub Actions çalıştırma kimliği</label>
          <input id={`${uid}-run`} type="text" inputMode="numeric" autoComplete="off" spellCheck={false} value={runId} aria-invalid={invalidField === "runId"}
            aria-describedby={`${uid}-err`} onChange={(e) => edited(setRunId)(e.target.value)} />
        </div>
        <div className="field">
          <label htmlFor={`${uid}-att`}>Deneme numarası (1–{MAX_RUN_ATTEMPT})</label>
          <input id={`${uid}-att`} type="text" inputMode="numeric" autoComplete="off" spellCheck={false} value={attempt} aria-invalid={invalidField === "runAttempt"}
            aria-describedby={`${uid}-err`} onChange={(e) => edited(setAttempt)(e.target.value)} />
        </div>
        <p className="actions">
          <button type="submit" disabled={pending}>GitHub CI Kanıtını Denetle</button>
        </p>
      </form>
      <div id={`${uid}-err`} aria-live="polite">
        {pending && <p className="muted small">Denetim sürüyor; GitHub’dan yanıt bekleniyor. Girdiyi değiştirirseniz bu yanıt gösterilmez.</p>}
        {invalidField !== null && <p className="banner banner-error small" role="alert">{INPUT_ERROR[invalidField]} Hiçbir istek gönderilmedi.</p>}
      </div>
      {view.kind === "error" && (
        <section ref={resultRef} tabIndex={-1} aria-label="GitHub CI denetim hatası" className="banner banner-error" role="alert">
          <strong>Denetim yapılamadı.</strong> {view.error.message} <code>{view.error.code}</code>
          <span className="muted"> Hiçbir CI sonucu üretilmedi.</span>
        </section>
      )}
      {view.kind === "shown" && (
        <section ref={resultRef} tabIndex={-1} aria-label="GitHub CI denetim sonucu" className="cp-result">
          <Result view={view.view} />
        </section>
      )}
    </section>
  );
}
