import { useEffect, useState } from "react";
import { api, toAppError } from "../api";
import { ErrorNotice, useLoad } from "../components/common";
import { compatLabel, platformLabel, surfaceLabel } from "../labels";
import type { AppError, InstallPlan, PlanRequest } from "../types";

const PLATFORMS = ["windows-x64", "macos-arm64"] as const;
const SURFACES = ["local-cli", "claude-code-terminal", "claude-desktop-code", "antigravity-ide"] as const;
const SCOPES: Record<string, string> = { user: "Bu kullanıcı için", project: "Tek bir proje için" };

const OPERATION: Record<string, string> = {
  "fetch-release-artifact": "Sürüm dosyasını indir",
  "verify-artifact-digest": "Dosyanın özetini doğrula",
  "stage-private-copy": "Özel hazırlık kopyası al",
  "activate-staged-artifact": "Hazırlanan dosyayı etkinleştir",
  "install-pinned-package": "Sabitlenmiş paketi kur",
  "register-native-plugin": "Eklentiyi ajanın yöneticisine kaydet",
  "write-managed-instruction-block": "Talimat dosyasına yönetilen blok yaz",
};
const MISSING: Record<string, string> = {
  expectedSha256: "beklenen SHA-256 özeti", packageName: "doğrulanmış paket adı", version: "sabitlenmiş sürüm", pluginId: "eklenti kimliği",
};
const REVERSIBILITY: Record<string, string> = {
  host_removable: "Ajanın kendi aracıyla kaldırılabilir (bıraktığı veriler kalabilir)",
  version_switch: "Tasarım amacı: önceki sürüme dönüş (bu sürümde uygulanmadı)",
  nothing_to_undo: "Geri alınacak kalıcı bir değişiklik yok",
  unknown: "Geri alınabilirliği bilinmiyor",
};
const PREREQ: Record<string, string> = { met: "Karşılandı", unmet: "Karşılanmadı", unknown: "Bilinmiyor" };
const PREREQ_ICON: Record<string, string> = { met: "✓", unmet: "✕", unknown: "?" };
const BASIS: Record<string, string> = {
  current_scan: "güncel tarama kanıtı", no_current_evidence: "güncel ve uygulanabilir kanıt yok — bilinmiyor, “yok” demek değil",
  publisher_declared: "yayıncı beyanı", not_checked: "denetlenmedi",
};
const PIN: Record<string, string> = {
  none: "katalogda commit bildirilmemiş", malformed: "bildirilen commit geçersiz (yok sayıldı)",
  declared_unverified: "commit yalnızca BEYAN edilmiş; kaynağın bu commit’e ait olduğu doğrulanmadı",
};
const SOURCE: Record<string, string> = {
  observed_scan: "son tarama kaydından", publisher_declared: "yayıncının beyanı (doğrulanmadı)", not_checked: "denetlenmedi",
};

export function PlanPage() {
  const catalog = useLoad(api.listCatalog);
  const info = useLoad(api.getAppInfo);
  const [entryId, setEntryId] = useState("");
  const [platform, setPlatform] = useState<string>("windows-x64");
  const [surface, setSurface] = useState<string>("local-cli");
  const [scope, setScope] = useState<string>("user");
  const [alias, setAlias] = useState<string>("");
  const projects = useLoad(api.listProjects);
  const [method, setMethod] = useState<string>("");
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);

  const entries = (catalog.data ?? []).filter((e) => e.removedAt === null);

  // Default the platform to this computer when it is one of the two supported targets.
  useEffect(() => {
    const os = info.data?.platform;
    if (os === "macos") setPlatform("macos-arm64");
    if (os === "windows") setPlatform("windows-x64");
  }, [info.data]);

  const show = async (chosenMethod: string) => {
    if (!entryId) return;
    setBusy(true);
    setError(null);
    const request: PlanRequest = { entryId, platformTarget: platform, agentSurface: surface, scope, methodId: chosenMethod || null, projectAlias: scope === "project" ? alias : null };
    try {
      setPlan(await api.planInstall(request));
    } catch (e) {
      setPlan(null);
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  // Any change of the question makes the previous answer stale: it is removed, not left on screen.
  const changed = (set: (v: string) => void) => (v: string) => {
    set(v);
    setPlan(null);
  };

  return (
    <>
      <div className="banner banner-synthetic" role="note">
        <strong>ÖNİZLEME</strong> — Bu sayfa hiçbir şey kurmaz, indirmez, çalıştırmaz veya değiştirmez; üçüncü taraf araçlara ve ajan ayarlarına dokunmaz.
        Gösterilen adımlar yalnızca “kurulsaydı neler olurdu” açıklamasıdır; bir izin veya onay değildir.
      </div>

      <form
        className="panel"
        aria-labelledby="h-plan-form"
        onSubmit={(ev) => { ev.preventDefault(); void show(method); }}
      >
        <h2 id="h-plan-form">Hangi araç için plan görmek istiyorsunuz?</h2>
        <div className="field">
          <label htmlFor="plan-entry">Araç</label>
          <select id="plan-entry" value={entryId} onChange={(e) => { changed(setEntryId)(e.target.value); setMethod(""); }}>
            <option value="">Seçin…</option>
            {entries.map((e) => <option key={e.entryId} value={e.entryId}>{e.displayName}</option>)}
          </select>
        </div>
        <div className="field">
          <label htmlFor="plan-platform">Hedef bilgisayar türü</label>
          <select id="plan-platform" value={platform} onChange={(e) => changed(setPlatform)(e.target.value)}>
            {PLATFORMS.map((p) => <option key={p} value={p}>{platformLabel(p)}</option>)}
          </select>
        </div>
        <div className="field">
          <label htmlFor="plan-surface">Hedef ajan yüzeyi</label>
          <select id="plan-surface" value={surface} onChange={(e) => changed(setSurface)(e.target.value)}>
            {SURFACES.map((s) => <option key={s} value={s}>{surfaceLabel(s)}</option>)}
          </select>
        </div>
        <div className="field">
          <label htmlFor="plan-scope">Kapsam</label>
          <select id="plan-scope" value={scope} onChange={(e) => changed(setScope)(e.target.value)}>
            {Object.entries(SCOPES).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
          </select>
        </div>
        {scope === "project" && (
          <div className="field">
            <label htmlFor="plan-project">Proje</label>
            <select id="plan-project" value={alias} onChange={(e) => changed(setAlias)(e.target.value)}>
              <option value="">Seçin…</option>
              {(projects.data ?? []).map((p) => <option key={p.alias} value={p.alias}>{p.displayName}</option>)}
            </select>
          </div>
        )}
        {plan && plan.methods.length > 0 && (
          <div className="field">
            <label htmlFor="plan-method">Kurulum yöntemi</label>
            <select id="plan-method" value={method} onChange={(e) => { setMethod(e.target.value); void show(e.target.value); }}>
              <option value="">Otomatik (bu platformda desteklenen ilk yöntem)</option>
              {plan.methods.map((m) => (
                <option key={m.methodId} value={m.methodId}>{m.label}{m.supported ? "" : " (tarif türü yok)"}</option>
              ))}
            </select>
          </div>
        )}
        <p>
          <button type="submit" disabled={!entryId || busy || (scope === "project" && alias === "")}>Planı göster (hiçbir şey kurulmaz)</button>
        </p>
        {error && <ErrorNotice error={error} />}
      </form>

      {plan && <PlanView plan={plan} />}
    </>
  );
}

function PlanView({ plan }: { plan: InstallPlan }) {
  return (
    <section className="panel" aria-labelledby="h-plan-result">
      <h2 id="h-plan-result">{plan.entry.displayName} için kurulum planı (ön izleme)</h2>
      <p role="note" className="banner banner-synthetic">{plan.statement}</p>

      <h3>Uygunluk</h3>
      <p>
        <strong role="status">⛔ Bu plan uygulanamaz.</strong> Bu sürümde hiçbir araç kurulamaz; ayrıca aşağıdaki nedenler de geçerlidir:
      </p>
      <ul aria-label="Engelleme nedenleri">
        {plan.eligibility.blockers.map((b) => <li key={b.code}>{b.text} <code className="small">{b.code}</code></li>)}
      </ul>
      {plan.eligibility.warnings.length > 0 && (
        <>
          <h3>Uyarılar</h3>
          <ul aria-label="Uyarılar">{plan.eligibility.warnings.map((w) => <li key={w.code}>{w.text}</li>)}</ul>
        </>
      )}

      <h3>Hedef</h3>
      <ul aria-label="Hedef">
        <li>Araç: {plan.entry.displayName} · kaynak: {plan.entry.source} · sürüm etiketi: {plan.entry.revision}</li>
        <li>Platform: {platformLabel(plan.target.platformTarget)} · ajan yüzeyi: {surfaceLabel(plan.target.agentSurface)} · kapsam: {SCOPES[plan.target.scope] ?? plan.target.scope}</li>
        <li>
          {plan.target.thisComputerMatches
            ? "Seçilen platform bu bilgisayarınkiyle aynı."
            : "Seçilen platform bu bilgisayarınkinden FARKLI: plan yalnızca incelenebilir."}
        </li>
        <li>Uyumluluk (yayıncı beyanı, test edilmedi): {compatLabel(plan.target.catalogCompatibility)}</li>
        <li>Katalog incelemesi: {plan.entry.reviewState === "metadata-reviewed" ? "künye incelendi" : "bağımsız inceleme yok"} · kaynak sabitleme: {PIN[plan.entry.sourcePin] ?? plan.entry.sourcePin}</li>
      </ul>

      <h3>Ön koşullar</h3>
      {plan.prerequisites.length === 0 ? <p className="muted">Bu plan için ön koşul listelenmedi.</p> : (
        <ul aria-label="Ön koşullar">
          {plan.prerequisites.map((p, i) => (
            <li key={`${p.id}-${i}`}>
              <span aria-hidden="true">{PREREQ_ICON[p.state] ?? "?"}</span> <strong>{PREREQ[p.state] ?? p.state}</strong> — {p.text}{" "}
              <span className="muted small">({SOURCE[p.source] ?? p.source}; {BASIS[p.basis] ?? p.basis})</span>
            </li>
          ))}
        </ul>
      )}

      <h3>Önerilen adımlar — UYGULANMAYACAK</h3>
      {plan.steps.length === 0 ? (
        <p>Bu seçim için tipli bir adım üretilemedi; hiçbir komut tahmin edilmez veya README’den alınmaz.</p>
      ) : (
        <ol aria-label="Önerilen adımlar">
          {plan.steps.map((s) => (
            <li key={s.index}>
              <strong>{OPERATION[s.kind] ?? s.kind}</strong> — {s.summary}
              <ul className="small">
                {s.missingParameters.length > 0 && <li>Eksik bilgi (bu yüzden tipli işlem yazılamıyor): {s.missingParameters.map((m) => MISSING[m] ?? m).join(", ")}</li>}
                <li>{s.network === "yes" ? "Ağ kullanımı gerekecekti." : "Ağ kullanımı gerekmeyecekti."}</li>
                {s.writeLabels.map((w) => <li key={w}>Yazılacağı yer: {w}</li>)}
                <li>{REVERSIBILITY[s.reversibility] ?? s.reversibility}</li>
                {s.admin === "maybe" && <li>Yönetici izni gerekip gerekmediği önceden bilinemez.</li>}
              </ul>
            </li>
          ))}
        </ol>
      )}

      <h3>Değişecek yapılandırma</h3>
      {plan.configChanges.length === 0 ? <p className="muted">Hiçbir ajan yapılandırma dosyası değişmeyecekti (ya da bilinmiyor).</p> : (
        <ul aria-label="Yapılandırma değişiklikleri">{plan.configChanges.map((c) => <li key={c.area}>{c.text}</li>)}</ul>
      )}

      <h3>İzinler, ağ, veri</h3>
      <ul aria-label="İzinler ve veri">
        {plan.permissions.map((p) => <li key={p}>{p}</li>)}
        <li>Ağ: {plan.network.plannedUse ? `gerekecekti (${plan.network.destinations.join("; ")})` : "gerekmeyecekti"}. Bu önizleme ağa bağlanmadı.</li>
        <li>Telemetri: {plan.telemetry.text}</li>
        <li>Veri ve kimlik bilgileri: {plan.dataAndCredentials.text}</li>
        <li>Disk kullanımı: {plan.diskUsage.text}</li>
      </ul>

      <h3>Çakışmalar</h3>
      {plan.conflicts.length === 0 ? <p className="muted">Çakışma kaydı yok.</p> : (
        <ul aria-label="Çakışmalar">{plan.conflicts.map((c) => <li key={c.code}>{c.text}</li>)}</ul>
      )}

      <h3>Kaldırma ve geri alma</h3>
      <ul aria-label="Kaldırma ve geri alma">
        <li>Kaldırma: {plan.uninstall.text}</li>
        <li><strong>Geri alma: yok.</strong> {plan.rollback.text}</li>
      </ul>

      {plan.declaredDependencies.length > 0 && (
        <>
          <h3>Yayıncının bildirdiği bağımlılıklar (doğrulanmadı)</h3>
          <ul aria-label="Yayıncı beyanları">{plan.declaredDependencies.map((d) => <li key={d}>{d}</li>)}</ul>
        </>
      )}

      <h3>Bilinmeyenler</h3>
      <ul aria-label="Bilinmeyenler">{plan.unknowns.map((u) => <li key={u}>{u}</li>)}</ul>

      <details>
        <summary className="small">Plan künyesi</summary>
        <p className="small muted">Plan kimliği: <code>{plan.planId.slice(0, 16)}…</code> · biçim sürümü {plan.schemaVersion} · planlayıcı {plan.plannerVersion} · durum: yalnızca önizleme</p>
      </details>
    </section>
  );
}
