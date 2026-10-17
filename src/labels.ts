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

const REVIEW: Record<string, string> = { unreviewed: "İncelenmemiş" };
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
  accepted: "Kabul edildi",
  rejected: "Reddedildi",
  later: "Sonra",
  disabled: "Devre dışı",
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
