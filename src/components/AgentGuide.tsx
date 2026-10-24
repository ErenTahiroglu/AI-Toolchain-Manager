// AGENT-01 - "Diğer ajanlar ve MCP/Skills kullanım kılavuzu": one static, shared, manual guide. It reads no file, writes no file, calls no IPC command, opens no network
// connection, stores nothing, copies nothing and changes no preference. The only interaction is a name lookup against a short, documented example table; any other
// name is answered "MANUAL_VERIFY". All content is in src/agentGuide.ts and src/agentHosts.ts.
import { useId, useState, type FormEvent } from "react";
import { AGENT_HOST_SURFACES, CAPABILITIES, CAPABILITY_LABEL } from "../agentHosts";
import { CONCEPTS, GUIDE_EXAMPLES, GUIDE_STEPS, KIND_LABEL, NO_UNIVERSAL_FORMAT, classifyName, type Classification } from "../agentGuide";
import { surfaceLabel } from "../labels";

const PREVIEW_TEXT = {
  "json-diff": "JSON önizlemesi (salt okunur)",
  "toml-not-implemented": "TOML önizlemesi uygulanmadı — yalnızca elle yönerge",
  none: "Önizleme yok",
} as const;

export function AgentGuide() {
  const uid = useId();
  const [name, setName] = useState("");
  const [result, setResult] = useState<Classification | null>(null);
  const hosts = AGENT_HOST_SURFACES.filter((h) => h.kind === "agent-host");

  const lookup = (ev: FormEvent) => {
    ev.preventDefault();
    setResult(classifyName(name));
  };

  return (
    <details className="agent-guide">
      <summary>Diğer ajanlar ve MCP/Skills kullanım kılavuzu — elle, salt okunur</summary>
      <p className="small">
        Bu bölüm bir <strong>kılavuzdur</strong>: dosya okumaz veya yazmaz, ağa bağlanmaz, hesap veya anahtar istemez, hiçbir şey kurmaz ve tercihinizi değiştirmez. Uygulamanın
        yönetmediği her ajan istemcisi için <strong>aynı elle iş akışı</strong> geçerlidir; her marka veya sağlayıcı için ayrı bir bağdaştırıcı yoktur.
      </p>
      <p role="note" className="small"><strong>{NO_UNIVERSAL_FORMAT}</strong></p>

      <h4>Elle doğrulama iş akışı</h4>
      <ol className="small">
        {GUIDE_STEPS.map((s) => (
          <li key={s.id}><strong>{s.title.replace(/^\d+\.\s*/, "")}.</strong> {s.body}</li>
        ))}
      </ol>

      <h4>Farklı şeyler, farklı yerler</h4>
      <dl className="small ag-list" aria-label="MCP sunucusu, skill, eklenti, yönerge dosyası, sağlayıcı ve çalışma ortamı arasındaki farklar">
        {CONCEPTS.map((c) => (
          <div key={c.thing}><dt>{c.thing}</dt><dd>{c.what} <span className="muted">Paylaşılır mı? {c.shared}</span></dd></div>
        ))}
      </dl>

      <h4>Uygulamanın seçilebilir hedef yaptığı ajan yüzeyleri</h4>
      <ul className="small ag-hosts" aria-label="Seçilebilir ajan yüzeyleri: proje dosyası, biçim ve belgelenmiş yetenekler">
        {hosts.map((h) => (
          <li key={h.agentSurface}>
            <strong>{surfaceLabel(h.agentSurface)}</strong>
            <dl className="facts small">
              <dt>Proje dosyası (kökten göreli)</dt><dd>{h.relativeTargetPath ? <code>{h.relativeTargetPath}</code> : "belgelenmiş proje dosyası yok"}</dd>
              <dt>Biçim</dt><dd>{h.configFormat ? h.configFormat.toUpperCase() : "—"}</dd>
              <dt>Önizleme</dt><dd>{PREVIEW_TEXT[h.previewKind]}</dd>
              <dt>Belgelenmiş yetenekler</dt>
              <dd>{CAPABILITIES.filter((c) => h.capabilities[c] === "documented").map((c) => CAPABILITY_LABEL[c]).join(", ") || "yok"}</dd>
              <dt>Değerlendirilmedi</dt>
              <dd>{CAPABILITIES.filter((c) => h.capabilities[c] !== "documented").map((c) => CAPABILITY_LABEL[c]).join(", ") || "—"}</dd>
            </dl>
          </li>
        ))}
      </ul>
      <p className="small muted">
        “Değerlendirilmedi” bir yokluk iddiası değildir: yalnızca bu çalışmada okunan yayıncı sayfalarında bulunmadığı anlamına gelir. İşletim sistemi (Windows x64 / macOS Apple Silicon)
        bu listeden bağımsızdır; uyumluluk her araç için katalog kanıtından gelir ve bilinmeyen, bilinmeyen kalır.
      </p>

      <h4>Örnekler (yalnızca açıklayıcı)</h4>
      <dl className="small ag-list" aria-label="Gemini CLI, Grok/xAI, Groq, DeepSeek ve yönetilen istemciler: sınıflandırma">
        {GUIDE_EXAMPLES.map((e) => (
          <div key={e.id}><dt>{e.name}</dt><dd><strong>{KIND_LABEL[e.kind]}.</strong> {e.note} <span className="muted">(kaynak: {e.evidenceIds.join(", ")})</span></dd></div>
        ))}
      </dl>

      <form onSubmit={lookup} aria-labelledby={`${uid}-h`} className="field">
        <h4 id={`${uid}-h`}>Bir adı sınıflandırın</h4>
        <label htmlFor={`${uid}-n`}>Ad (ör. bir istemci veya sağlayıcı adı)</label>
        <input id={`${uid}-n`} type="text" autoComplete="off" spellCheck={false} maxLength={60} value={name} onChange={(e) => { setName(e.target.value); setResult(null); }} />
        <p className="actions"><button type="submit" className="secondary">Sınıflandır<span className="sr-only"> (yalnızca yukarıdaki belgelenmiş örnek tablosuna bakar; hiçbir şey kaydedilmez)</span></button></p>
        <div role="status" aria-live="polite">
          {result && (
            <p className="small">
              <strong>{KIND_LABEL[result.kind]}</strong> <code>{result.verdict}</code> — {result.message}
            </p>
          )}
        </div>
      </form>
    </details>
  );
}
