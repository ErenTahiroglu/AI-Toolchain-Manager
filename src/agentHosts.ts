// AGENT-01 - a small, declarative registry of the agent-client surfaces the application can target. Pure data: no I/O, no IPC, no clock.
//
// Three different things are NEVER mixed here, and a model/API provider's brand name never selects or overrides any field below:
//   * an AGENT CLIENT / HOST SURFACE: the program (CLI, IDE extension, desktop app) that can discover MCP tools and/or skills. This registry is about these;
//   * a MODEL / API PROVIDER: a service a host calls (DeepSeek API, GroqCloud, xAI/Grok, Gemini API ...). Not a host, has no project MCP file, and is not a row here;
//   * a PROTOCOL / CAPABILITY: MCP over stdio or HTTP, skills (SKILL.md), host plugins, text instruction files (AGENTS.md / CLAUDE.md). They are not
//     interchangeable formats; each entry states, per capability, only what a publisher page read for this phase documents ("documented") - everything else is
//     "not-assessed", which is NOT a claim of absence.
// The operating system is a separate dimension of the stored preference (project + catalog entry + OS + surface) and of the catalog's compatibility evidence; nothing
// in this registry says that a host works on Windows or macOS. Unknown or unsupported means: no preview, no accepted preference, no write.
// Sources: docs/agent-01/CODEX_SOURCE_AND_SCOPE.md and HOST_PROVIDER_PROTOCOL_MATRIX.md (R45-R49 in docs/RESEARCH-SOURCES.md).

export const CAPABILITIES = ["mcp_stdio", "mcp_http", "skills", "plugins", "project_instructions"] as const;
export type Capability = (typeof CAPABILITIES)[number];
export type CapabilityStatus = "documented" | "not-assessed";

export const CAPABILITY_LABEL: Record<Capability, string> = {
  mcp_stdio: "MCP (yerel süreç / stdio)",
  mcp_http: "MCP (HTTP)",
  skills: "Skills (SKILL.md)",
  plugins: "Ajan eklentileri",
  project_instructions: "Proje yönerge dosyası",
};

export type ConfigFormat = "json" | "toml";
/** What the read-only configuration preview does for this surface. */
export type PreviewKind = "json-diff" | "toml-not-implemented" | "none";

export interface AgentHostSurface {
  /** The application's agent-surface id (a member of TARGET_SURFACES; the Rust allowlist is `projects::SURFACES`). */
  agentSurface: string;
  /** The client product family. Several surfaces of one family are NOT interchangeable: a surface is only what it says. */
  clientHostId: string;
  kind: "agent-host" | "program-lookup";
  /** "project" only where a publisher page documents a project-scoped MCP file for exactly this surface; otherwise "none-documented". */
  configScope: "project" | "none-documented";
  configFormat: ConfigFormat | null;
  /** Relative to the selected project's root; fixed text, never built from input; null when no project file is documented for this surface. */
  relativeTargetPath: string | null;
  /** Plain statement of the condition under which the host reads that file (null when none is documented). */
  scopeCondition: string | null;
  previewKind: PreviewKind;
  capabilities: Record<Capability, CapabilityStatus>;
  /** True exactly when the installer-side plan layer (src-tauri plan.rs `SURFACES`) knows this surface. A project target is not an installer target. */
  planSupported: boolean;
  evidenceIds: readonly string[];
  /** Files of the same client that are deliberately never read, previewed or written. */
  untouched: readonly string[];
}

const NOT_ASSESSED: Record<Capability, CapabilityStatus> = {
  mcp_stdio: "not-assessed",
  mcp_http: "not-assessed",
  skills: "not-assessed",
  plugins: "not-assessed",
  project_instructions: "not-assessed",
};

/** Same order as TARGET_SURFACES (a test pins it). */
export const AGENT_HOST_SURFACES: readonly AgentHostSurface[] = [
  {
    agentSurface: "claude-code-terminal",
    clientHostId: "claude-code",
    kind: "agent-host",
    configScope: "project",
    configFormat: "json",
    relativeTargetPath: ".mcp.json",
    scopeCondition: "Etkileşimli oturumlarda proje sunucuları için onay istenir; “-p” ve Agent SDK çalıştırmalarında istem gösterilemez ve sunucular sormadan yüklenir.",
    previewKind: "json-diff",
    capabilities: { ...NOT_ASSESSED, mcp_stdio: "documented", mcp_http: "documented", project_instructions: "documented" },
    planSupported: true,
    evidenceIds: ["R45", "R46"],
    untouched: ["~/.claude.json (yerel ve kullanıcı kapsamı)", ".claude/settings.json", ".claude/settings.local.json", "CLAUDE.md"],
  },
  {
    agentSurface: "claude-desktop-code",
    clientHostId: "claude-desktop",
    kind: "agent-host",
    configScope: "none-documented",
    configFormat: null,
    relativeTargetPath: null,
    scopeCondition: null,
    previewKind: "none",
    capabilities: NOT_ASSESSED,
    planSupported: true,
    evidenceIds: [],
    untouched: [],
  },
  {
    agentSurface: "antigravity-ide",
    clientHostId: "antigravity",
    kind: "agent-host",
    configScope: "project",
    configFormat: "json",
    relativeTargetPath: ".agents/mcp_config.json",
    scopeCondition: "Çalışma alanı dosyası yalnızca o çalışma alanı için geçerlidir. IDE'nin aynı dosya konumlarını kullandığı tek bir cümleye dayanır.",
    previewKind: "json-diff",
    capabilities: { ...NOT_ASSESSED, mcp_stdio: "documented", mcp_http: "documented" },
    planSupported: true,
    evidenceIds: ["R47"],
    untouched: ["~/.gemini/config/mcp_config.json (kullanıcı geneli)", "AGENTS.md", "uygulama içi MCP mağazası ayarları"],
  },
  {
    agentSurface: "codex-cli",
    clientHostId: "codex",
    kind: "agent-host",
    configScope: "project",
    configFormat: "toml",
    relativeTargetPath: ".codex/config.toml",
    scopeCondition:
      "Proje yapılandırması yalnızca proje GÜVENİLİR ise yüklenir; güvenilmeyen projede .codex/ katmanları yok sayılır. Proje kökünden çalışma dizinine kadar bulunan her .codex/config.toml yüklenir; aynı anahtarda çalışma dizinine en yakın dosya kazanır.",
    previewKind: "toml-not-implemented",
    capabilities: { ...NOT_ASSESSED, mcp_stdio: "documented", mcp_http: "documented", plugins: "documented", project_instructions: "documented" },
    planSupported: false,
    evidenceIds: ["R48", "R49"],
    untouched: [
      "~/.codex/config.toml (kullanıcı yapılandırması; model sağlayıcıları ve kimlik bilgileri burada yaşar)",
      "~/.codex/auth.json ve işletim sistemi anahtar zinciri",
      "AGENTS.md",
      "ChatGPT masaüstü uygulaması / IDE eklentisi ayarları (bu hedef değildir)",
    ],
  },
  // AGENT-02 (Issue #53). Six more surfaces, each a SEPARATE row: a publisher statement about one surface of a client is never copied to another surface of the
  // same client (Codex CLI vs Codex VS Code, Cline CLI vs Cline VS Code vs Cline JetBrains, Kiro CLI vs Kiro IDE). Nothing here is runtime evidence: "documented" means a
  // publisher page read on 2026-11-14 says it (docs/agent-02/SOURCE_EVIDENCE.md); no program was installed, started or probed. No new surface has a preview: the
  // configuration preview (src/configPlan.ts) answers UNSUPPORTED_SURFACE for every one of them, and none is a member of the install-plan list (plan.rs `SURFACES`).
  {
    agentSurface: "codex-vscode",
    clientHostId: "codex",
    kind: "agent-host",
    configScope: "none-documented",
    configFormat: null,
    relativeTargetPath: null,
    scopeCondition: null,
    previewKind: "none",
    capabilities: NOT_ASSESSED,
    planSupported: false,
    evidenceIds: ["R58"],
    untouched: [
      "~/.codex/config.toml ve .codex/config.toml (Codex CLI hedefinin dosyaları; bu yüzeyin proje dosyası belgelenmiş sayılmaz)",
      "~/.codex/auth.json ve işletim sistemi anahtar zinciri",
      "VS Code kullanıcı ve çalışma alanı ayarları",
      "Cursor, Windsurf ve diğer VS Code uyumlu düzenleyiciler (bu hedef değildir), Xcode ve JetBrains bütünleştirmeleri",
    ],
  },
  {
    agentSurface: "cline-cli",
    clientHostId: "cline",
    kind: "agent-host",
    configScope: "none-documented",
    configFormat: null,
    relativeTargetPath: null,
    scopeCondition: null,
    previewKind: "none",
    capabilities: { ...NOT_ASSESSED, mcp_stdio: "documented", mcp_http: "documented" },
    planSupported: false,
    evidenceIds: ["R59"],
    untouched: ["~/.cline/data/settings/cline_mcp_settings.json (kullanıcı düzeyi; CLINE_MCP_SETTINGS_PATH ile değiştirilebilir; proje dosyası DEĞİLDİR)", "Cline kimlik bilgileri ve sağlayıcı anahtarları"],
  },
  {
    agentSurface: "cline-vscode",
    clientHostId: "cline",
    kind: "agent-host",
    configScope: "none-documented",
    configFormat: null,
    relativeTargetPath: null,
    scopeCondition: null,
    previewKind: "none",
    capabilities: NOT_ASSESSED,
    planSupported: false,
    evidenceIds: [],
    untouched: ["Cline CLI dosyaları (başka bir yüzey)", "VS Code kullanıcı ve çalışma alanı ayarları", "Cline eklentisinin MCP ayar dosyası (yolu bu çalışmada okunmadı)"],
  },
  {
    agentSurface: "cline-jetbrains",
    clientHostId: "cline",
    kind: "agent-host",
    configScope: "none-documented",
    configFormat: null,
    relativeTargetPath: null,
    scopeCondition: null,
    previewKind: "none",
    capabilities: NOT_ASSESSED,
    planSupported: false,
    evidenceIds: [],
    untouched: ["Cline CLI ve Cline VS Code dosyaları (başka yüzeyler)", "JetBrains IDE ayarları", "Cline JetBrains bütünleştirmesinin MCP ayar dosyası (yolu bu çalışmada okunmadı)"],
  },
  {
    agentSurface: "kiro-cli",
    clientHostId: "kiro",
    kind: "agent-host",
    configScope: "project",
    configFormat: "json",
    relativeTargetPath: ".kiro/settings/mcp.json",
    scopeCondition:
      "Yayıncı sayfası bu dosyayı çalışma alanı düzeyi olarak belgeler; sayfa IDE, CLI ve Web için ortaktır. Öncelik: aracı yapılandırması > çalışma alanı > kullanıcı. Dosya kaydedilince sunucular otomatik yeniden bağlanır; “autoApprove” içindeki “*” tüm araçları sormadan onaylar; kimlik bilgileri dosyada bulunabilir.",
    previewKind: "none",
    capabilities: { ...NOT_ASSESSED, mcp_stdio: "documented", mcp_http: "documented" },
    planSupported: false,
    evidenceIds: ["R60"],
    untouched: [
      "~/.kiro/settings/mcp.json (kullanıcı düzeyi)",
      "Kiro aracı (agent) JSON dosyaları ve içindeki mcpServers alanı",
      "autoApprove, oauth ve headers alanları (hiçbir zaman okunmaz, önizlenmez, yazılmaz)",
      "Kiro IDE ayarları (başka bir yüzey)",
    ],
  },
  {
    agentSurface: "kiro-ide",
    clientHostId: "kiro",
    kind: "agent-host",
    configScope: "project",
    configFormat: "json",
    relativeTargetPath: ".kiro/settings/mcp.json",
    scopeCondition:
      "Yayıncı sayfası bu dosyayı çalışma alanı düzeyi olarak belgeler; sayfa IDE, CLI ve Web için ortaktır. Öncelik: aracı yapılandırması > çalışma alanı > kullanıcı. Dosya kaydedilince sunucular otomatik yeniden bağlanır; “autoApprove” içindeki “*” tüm araçları sormadan onaylar; kimlik bilgileri dosyada bulunabilir.",
    previewKind: "none",
    capabilities: { ...NOT_ASSESSED, mcp_stdio: "documented", mcp_http: "documented" },
    planSupported: false,
    evidenceIds: ["R60"],
    untouched: [
      "~/.kiro/settings/mcp.json (kullanıcı düzeyi)",
      "Kiro aracı (agent) JSON dosyaları ve içindeki mcpServers alanı",
      "autoApprove, oauth ve headers alanları (hiçbir zaman okunmaz, önizlenmez, yazılmaz)",
      "Kiro CLI ayarları (başka bir yüzey)",
    ],
  },
  {
    agentSurface: "local-cli",
    clientHostId: "local-programs",
    kind: "program-lookup",
    configScope: "none-documented",
    configFormat: null,
    relativeTargetPath: null,
    scopeCondition: null,
    previewKind: "none",
    capabilities: NOT_ASSESSED,
    planSupported: true,
    evidenceIds: [],
    untouched: [],
  },
];

export const agentHostFor = (surface: string): AgentHostSurface | null => AGENT_HOST_SURFACES.find((h) => h.agentSurface === surface) ?? null;

/** The surfaces the installer-side plan layer knows (everything but the project-only targets: Codex, Cline and Kiro). */
export const planSurfaces = (surfaces: readonly string[]): string[] => surfaces.filter((s) => agentHostFor(s)?.planSupported === true);
