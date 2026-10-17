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
