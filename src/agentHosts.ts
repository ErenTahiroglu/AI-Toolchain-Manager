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

/** The surfaces the installer-side plan layer knows (everything but the project-only Codex target). */
export const planSurfaces = (surfaces: readonly string[]): string[] => surfaces.filter((s) => agentHostFor(s)?.planSupported === true);
