// AGENT-01 - the content of "Diğer ajanlar ve MCP/Skills kullanım kılavuzu": ONE shared, manual workflow for every client the application does not manage. Pure data and two
// tiny pure functions. No I/O, no IPC, no network, no clock. Nothing here is an adapter, a config template or a compatibility claim.
//
// What it refuses to do: infer a host, a file path or a config format from a provider's brand name. A name that is not in the short, documented example table below is
// "uncertain" and the answer is MANUAL_VERIFY - never a made-up path, schema, "compatible" or "installed" label.
// Sources of the examples: docs/agent-01/HOST_PROVIDER_PROTOCOL_MATRIX.md (R48-R53 in docs/RESEARCH-SOURCES.md).
import { normalize } from "./workspace";

export type ItemKind =
  | "managed-agent-host" // a surface the application can target (see src/agentHosts.ts)
  | "example-agent-host" // a real agent host used only as a worked example in this guide; the application does not manage it
  | "model-api-provider" // a service a host calls; not a host, has no project MCP file of its own
  | "uncertain"; // anything else: verify by hand

export const KIND_LABEL: Record<ItemKind, string> = {
  "managed-agent-host": "Ajan istemcisi — uygulamada seçilebilir hedef",
  "example-agent-host": "Ajan istemcisi — yalnızca örnek (uygulama yönetmez)",
  "model-api-provider": "Model / API sağlayıcısı — ajan istemcisi değil",
  uncertain: "Belirsiz — MANUAL_VERIFY",
};

export interface GuideExample {
  id: string;
  name: string;
  /** Exact-match spellings (compared after case/diacritic-insensitive normalisation). No fuzzy matching: "grok" and "groq" are different things. */
  aliases: readonly string[];
  kind: ItemKind;
  /** One plain sentence: what it is and what it is NOT. */
  note: string;
  evidenceIds: readonly string[];
}

export const GUIDE_EXAMPLES: readonly GuideExample[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    aliases: ["claude code", "claude-code", "claude code terminal"],
    kind: "managed-agent-host",
    note: "Terminal ajanı. Uygulamada “Claude Code (terminal)” hedefidir; proje dosyası JSON (.mcp.json).",
    evidenceIds: ["R45", "R46"],
  },
  {
    id: "antigravity",
    name: "Antigravity IDE",
    aliases: ["antigravity", "antigravity ide"],
    kind: "managed-agent-host",
    note: "Uygulamada yalnızca “Antigravity IDE” hedefidir; proje dosyası JSON (.agents/mcp_config.json). Uygulama, CLI ve “2” arayüzleri birbirinin yerine geçmez.",
    evidenceIds: ["R47"],
  },
  {
    id: "codex",
    name: "Codex CLI",
    aliases: ["codex", "codex cli", "codex-cli"],
    kind: "managed-agent-host",
    note: "Uygulamada “Codex CLI” hedefidir; proje dosyası TOML (.codex/config.toml, yalnızca güvenilen projede). TOML önizlemesi uygulanmadı; yalnızca elle yönerge vardır.",
    evidenceIds: ["R48", "R49"],
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    aliases: ["gemini cli", "gemini-cli"],
    kind: "example-agent-host",
    note: "Bir ajan istemcisidir (JSON; kullanıcı ~/.gemini/settings.json, proje .gemini/settings.json). Yalnızca örnektir: uygulama onu yönetmez ve bir yazılım yüzeyi olarak eklenmemiştir. Yayıncı sayfası, bazı kullanıcı gruplarında Antigravity CLI ile değiştirildiğini belirtir.",
    evidenceIds: ["R50"],
  },
  {
    id: "gemini-api",
    name: "Gemini API",
    aliases: ["gemini api"],
    kind: "model-api-provider",
    note: "Bir model/API sağlayıcısıdır. “Gemini CLI” ile aynı şey değildir: ismin aynı olması bir ajan istemcisi olduğunu veya bir yapılandırma dosyası taşıdığını göstermez.",
    evidenceIds: ["R50"],
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    aliases: ["deepseek", "deep seek", "deepseek api"],
    kind: "model-api-provider",
    note: "Model/API sağlayıcısıdır. Yayıncı sayfası, DeepSeek'i Codex'te bir model sağlayıcısı olarak tanımlamayı anlatır (config.toml; sağlayıcı anahtarları proje dosyasında yok sayılır, R49). Sabit bir “proje MCP istemcisi” değildir ve API anahtarı bu uygulamaya girilmez.",
    evidenceIds: ["R51", "R49"],
  },
  {
    id: "grok",
    name: "Grok (xAI)",
    aliases: ["grok", "xai", "x.ai", "xai grok", "grok api", "grok (xai)"],
    kind: "model-api-provider",
    note: "xAI'nin model API'sidir. Groq ile aynı değildir. Yayıncı sayfası yerel bir MCP istemcisi veya proje dosyası belgelemez.",
    evidenceIds: ["R52"],
  },
  {
    id: "groq",
    name: "Groq (GroqCloud)",
    aliases: ["groq", "groqcloud", "groq cloud", "groq api", "groq (groqcloud)"],
    kind: "model-api-provider",
    note: "Hızlı model çıkarımı sağlayan bir API'dir. Grok (xAI) ile aynı değildir. Kendi API'sinde “uzak MCP” aracı kullanımı belgeler; bu, sağlayıcı tarafı bir özelliktir, yerel bir istemci yapılandırması değildir.",
    evidenceIds: ["R53"],
  },
];

export interface Classification {
  kind: ItemKind;
  /** Always "MANUAL_VERIFY" for an unknown name; "KNOWN_EXAMPLE" only for the documented example table. Never "COMPATIBLE" / "INSTALLED". */
  verdict: "KNOWN_EXAMPLE" | "MANUAL_VERIFY";
  example: GuideExample | null;
  message: string;
}

const squeeze = (s: string) => normalize(s).replace(/\s+/g, " ").trim();

/** Exact (normalised) alias lookup. Anything not in the table - including near misses such as "groc" or "grokk" - is uncertain. */
export function classifyName(input: string): Classification {
  const key = squeeze(input);
  if (key === "") {
    return { kind: "uncertain", verdict: "MANUAL_VERIFY", example: null, message: "Bir ad yazın. Ad tek başına hiçbir şeyi kanıtlamaz; tablo yalnızca belgelenmiş örnekleri tanır." };
  }
  const hit = GUIDE_EXAMPLES.find((e) => e.aliases.some((a) => squeeze(a) === key));
  if (hit) return { kind: hit.kind, verdict: "KNOWN_EXAMPLE", example: hit, message: hit.note };
  return {
    kind: "uncertain",
    verdict: "MANUAL_VERIFY",
    example: null,
    message: "Bu ad örnek tablosunda yok. Gerçek bir MCP/Skills istemcisi mi, bir model sağlayıcısı mı olduğunu adı tahmin ederek değil, yayıncının belgesinden elle doğrulayın (MANUAL_VERIFY). Dosya yolu, biçim veya uyumluluk uydurulmaz.",
  };
}

export interface GuideStep {
  id: string;
  title: string;
  body: string;
}

export const GUIDE_STEPS: readonly GuideStep[] = [
  {
    id: "classify",
    title: "1. Adı sınıflandırın",
    body: "Adı verilen şey ne? (a) MCP araçlarını ve/veya skill'leri bulup kullanabilen bir ajan istemcisi (CLI, IDE eklentisi, masaüstü uygulaması), (b) yalnızca yönerge dosyası okuyan bir istemci, (c) bir istemcinin çağırdığı model/API sağlayıcısı, yoksa (d) belirsiz mi? Marka adı bunu belirlemez: DeepSeek, Grok, Groq ve Gemini API sağlayıcıdır; Gemini CLI bir istemcidir.",
  },
  {
    id: "verify-support",
    title: "2. Resmî desteği doğrulayın",
    body: "Yayıncının kendi belgesinden şunları bulun: hangi MCP aktarımı (stdio, Streamable HTTP …), yapılandırma şeması (JSON mu, TOML mu, başka mı), dosyanın tam konumu ve hangi kapsamda (proje, kullanıcı, sistem) okunduğu. Dosya yolunu bir sağlayıcının adından veya başka bir istemciden tahmin etmeyin.",
  },
  {
    id: "separate-things",
    title: "3. Farklı şeyleri birbirinden ayırın",
    body: "MCP sunucuları, skill'ler (SKILL.md), ajan eklentileri, proje yönerge dosyaları (AGENTS.md, CLAUDE.md), barındırılan API modelleri ve çalışma ortamları farklı şeylerdir. Hiçbiri tek bir ortak “settings.json” biçimini paylaşmaz. Bir MCP sunucusu birden çok istemcide aynı olabilir; ama her istemcinin kendi dosyası, anahtar adları ve onay kuralları vardır.",
  },
  {
    id: "scope-and-risk",
    title: "4. Kapsamı ve riski belirleyin",
    body: "İşletim sistemi (Windows x64 / macOS Apple Silicon), CLI mı IDE mi, kullanıcı mı proje mi kapsamı, gereken ayrıcalıklar, API anahtarının nerede tutulduğu, hangi alt süreçlerin başlatıldığı ve hangi ağ bağlantılarının kurulduğu. Paket çalıştırıcıları (npx, uvx) her başlatmada kayıt defterinden kod indirir ve çalıştırır.",
  },
  {
    id: "manual-verify",
    title: "5. Kanıt eksikse MANUAL_VERIFY",
    body: "Resmî kanıt bulamadıysanız çıktı “MANUAL_VERIFY” olmalıdır: ne yapılandırma farkı, ne “uyumlu”, ne “kurulu”. Bilinmeyen hiçbir zaman desteklenen anlamına gelmez. Bu uygulama bilinmeyen bir istemci için dosya üretmez ve tercih olarak “kullanmak istiyorum” kaydetmez.",
  },
  {
    id: "examples",
    title: "6. Örnekler yalnızca açıklayıcıdır",
    body: "Gemini CLI, Grok/xAI, Groq ve DeepSeek aşağıda yalnızca nasıl düşünüleceğini göstermek için vardır. Her biri için ayrı bir uygulama bağdaştırıcısı yoktur ve gelecekteki adı geçmeyen istemciler de aynı elle iş akışını kullanır.",
  },
];

export interface ConceptRow {
  thing: string;
  what: string;
  shared: string;
}

export const CONCEPTS: readonly ConceptRow[] = [
  { thing: "MCP sunucusu", what: "Bir istemciye araç ve kaynak sunan program veya uzak adres (stdio ya da HTTP).", shared: "Sunucunun kendisi paylaşılabilir; her istemcinin tanım dosyası ve onay kuralları ayrıdır." },
  { thing: "Skill (SKILL.md)", what: "Bir istemcinin yükleyebildiği yeniden kullanılabilir yönergeler.", shared: "Biçim paylaşılabilir; nereye konacağı ve yüklenip yüklenmediği istemciye bağlıdır." },
  { thing: "Ajan eklentisi / pazaryeri", what: "Bir istemcinin kendi paketleme ve yönetim mekanizması.", shared: "İstemciye özgüdür; başka bir istemciye taşınmaz." },
  { thing: "Proje yönerge dosyası", what: "AGENTS.md, CLAUDE.md gibi metin yönergeleri.", shared: "Yapılandırma değildir; bir MCP sunucusu tanımlamaz." },
  { thing: "Model / API sağlayıcısı", what: "Bir istemcinin çağırdığı barındırılan model (API anahtarı ve adresle).", shared: "İstemci değildir; proje MCP dosyası yoktur. Sağlayıcı ayarı istemcinin KENDİ yapılandırmasındadır." },
  { thing: "Çalışma ortamı (runtime)", what: "Bir sunucuyu çalıştıran Node, Python, Docker gibi ortam.", shared: "Ayrı bir kurulum ve güven kararıdır." },
];

export const NO_UNIVERSAL_FORMAT =
  "Tüm istemciler için tek bir ortak settings.json veya tek bir birlikte çalışabilir yapılandırma biçimi yoktur. Aynı MCP sunucusu için Claude Code .mcp.json (JSON), Antigravity .agents/mcp_config.json (JSON) ve Codex .codex/config.toml (TOML) farklı anahtarlar ve kurallar kullanır.";
