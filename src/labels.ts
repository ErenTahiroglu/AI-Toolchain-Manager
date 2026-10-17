// Plain-language Turkish labels for technical values. Unknown values are shown as-is, never hidden.
import type { CheckStatus } from "./types";

const STATUS: Record<string, string> = {
  passed: "Başarılı",
  failed: "Başarısız",
  unknown: "Bilinmiyor",
  skipped: "Atlandı",
  blocked: "Engellendi",
};
export const statusLabel = (s: string) => STATUS[s] ?? s;

export const OVERALL: Record<Exclude<CheckStatus, "skipped">, string> = {
  passed: "Tüm kontroller başarılı",
  unknown: "Sorun bulunmadı, ancak bazı konular doğrulanamadı",
  failed: "Dikkat gerekiyor: en az bir kontrol başarısız",
};

const REVIEW: Record<string, string> = { unreviewed: "İncelenmemiş", quarantined: "Karantinada", "metadata-reviewed": "Künye incelendi", blocked: "Engelli" };
export const reviewLabel = (s: string) => REVIEW[s] ?? s;

const INSTALL: Record<string, string> = { not_managed: "Kurulu değil / yönetilmiyor" };
export const installLabel = (s: string) => INSTALL[s] ?? s;

const VALIDATION: Record<string, string> = { unknown: "Doğrulanmadı" };
export const validationLabel = (s: string) => VALIDATION[s] ?? s;

const COMPAT: Record<string, string> = {
  "vendor-declared": "Üretici belirtmiş (test edilmedi)",
  unknown: "Bilinmiyor",
  unsupported: "Desteklenmiyor",
};
export const compatLabel = (s: string) => COMPAT[s] ?? s;

const DECISION: Record<string, string> = {
  accepted: "Etkinleştirmek istiyorum",
  rejected: "Reddedildi",
  postponed: "Ertelendi",
  later: "Ertelendi",
  disabled: "Kapatıldı",
};
export const decisionLabel = (s: string) => DECISION[s] ?? s;

const PLATFORM: Record<string, string> = { "windows-x64": "Windows x64", "macos-arm64": "macOS Apple Silicon" };
export const platformLabel = (s: string) => PLATFORM[s] ?? s;

const SURFACE: Record<string, string> = {
  "local-cli": "Yerel komut satırı",
  "claude-code-terminal": "Claude Code (terminal)",
  "antigravity-ide": "Antigravity IDE",
  "claude-desktop-code": "Claude Desktop (Code)",
};
export const surfaceLabel = (s: string) => SURFACE[s] ?? s;

const CATEGORY: Record<string, string> = {
  "agent-plugin": "Ajan eklentisi",
  "mcp-server": "MCP sunucusu",
  "python-cli": "Python komut aracı",
  "native-cli": "Yerel komut aracı",
  "agent-skill": "Ajan yeteneği",
};
export const categoryLabel = (s: string) => CATEGORY[s] ?? s;

export const formatTime = (iso: string) => {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString("tr-TR");
};

const KIND: Record<string, string> = {
  executable_tool: "Çalıştırılabilir araç",
  agent_plugin: "Ajan eklentisi",
  skill_collection: "Yetenek koleksiyonu",
  prompt_source: "İstem kaynağı",
};
export const kindLabel = (s: string) => KIND[s] ?? s;

export const KIND_HELP: Record<string, string> = {
  executable_tool: "Bilgisayarınızda çalışan bir program veya komut satırı aracı.",
  agent_plugin: "Claude Code gibi bir yapay zekâ ajanına eklenen eklenti.",
  skill_collection: "Ajanların kullanabileceği yetenek (SKILL.md) dosyalarından oluşan kaynak.",
  prompt_source: "Hazır istem (prompt) metinleri içeren, yalnızca okunur kaynak.",
};

const POLICY: Record<string, string> = {
  "manual-only": "Yalnızca elle (bu sürümde hiçbir çalıştırma yok)",
  "blocked-until-scoped-authorization": "Kapsamlı yazılı yetki olmadan çalıştırma yasak",
};
export const policyLabel = (s: string) => POLICY[s] ?? s;

const METADATA: Record<string, string> = {
  "documented-snapshot": "Herkese açık belgelerden anlık görüntü (çevrimdışı)",
  "not-fetched": "Uzak veri çekilmedi — yalnızca bağlantıdan türetildi",
};
export const metadataLabel = (s: string) => METADATA[s] ?? s;

const LICENSE: Record<string, string> = { known: "Lisans bilinen", unknown: "Lisans bilinmiyor", "review-required": "Lisans ayrıca incelenmeli" };
export const licenseLabel = (s: string) => LICENSE[s] ?? s;

const SCOPE: Record<string, string> = { unknown: "Kapsam bilinmiyor (kurulum denetlenmedi)" };
export const scopeLabel = (s: string) => SCOPE[s] ?? s;

const PRESENCE: Record<string, string> = { present: "Bulundu", absent: "Bulunamadı", unknown: "Bilinmiyor", unsupported: "Desteklenmiyor" };
export const presenceLabel = (s: string) => PRESENCE[s] ?? s;

const CONFIGURATION: Record<string, string> = {
  configured: "Yapılandırılmış (başvuru var)",
  disabled: "Kapalı olarak işaretli",
  not_configured: "Yapılandırılmamış",
  referenced_not_present: "Başvuru var, dosyalar yok",
  malformed: "Yapılandırma dosyası bozuk",
  not_applicable: "Uygulanamaz",
  unknown: "Bilinmiyor",
};
export const configurationLabel = (s: string) => CONFIGURATION[s] ?? s;

const OBS_SCOPE: Record<string, string> = { global: "Genel (kullanıcı/bilgisayar)", project: "Proje", local: "Bu projeye özel (yerel)", unknown: "Bilinmiyor" };
export const obsScopeLabel = (s: string) => OBS_SCOPE[s] ?? s;

const CONFIDENCE: Record<string, string> = { high: "yüksek", medium: "orta", low: "düşük", none: "yok" };
export const confidenceLabel = (s: string) => CONFIDENCE[s] ?? s;

const METHOD: Record<string, string> = {
  "path-executable": "PATH klasörlerinde dosya adı",
  "native-install-location": "Claude Code'un belgelenmiş kurulum konumu",
  "legacy-local-install": "Eski yerel kurulum konumu",
  "claude-settings-file": "Claude Code ayar dosyası",
  "claude-plugin-declaration": "Claude Code eklenti kaydı",
  "claude-mcp-config": "Claude Code MCP yapılandırması",
  "no-detector": "Algılama yöntemi yok",
  "not-documented": "Belgelenmiş yöntem yok",
  "unsupported-platform": "Desteklenmeyen platform",
};
export const methodLabel = (s: string) => METHOD[s] ?? s;

const OBSERVED: Record<string, string> = {
  present_current: "Son taramada bulundu",
  absent_current: "Son taramada bulunamadı",
  unknown_current: "Son taramada belirlenemedi",
  previously_present: "Eski taramada bulunmuştu (güncel değil)",
  previously_absent: "Eski taramada bulunamamıştı (güncel değil)",
  previously_unknown: "Eski taramada belirlenememişti (güncel değil)",
  platform_mismatch: "Başka bir işletim sistemi için gözlem var (bu tercih için geçerli değil)",
  not_scanned: "Henüz taranmadı",
};
export const observedLabel = (s: string) => OBSERVED[s] ?? s;

export const subjectLabel = (subject: string, names: Record<string, string>) =>
  subject === "agent:claude-code" ? "Claude Code (ajan)" : subject === "agent:antigravity-ide" ? "Antigravity masaüstü IDE (ajan)" : names[subject] ?? subject;
