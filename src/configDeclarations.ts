// CONFIG-01 / CONFIG-02A - the registry of EXACT MCP declarations that the preview may turn into a before/after text for a (host file, catalog entry) pair.
//
// CONFIG-01 shipped it EMPTY on purpose: a publisher's install command, a README example, an `npx ...@latest` call or an API key placed in a URL is not an approved
// declaration. CONFIG-02A adds exactly ONE entry, for a real catalog entry on exactly one host file:
//
//   exa-mcp-server  x  Claude Code project `.mcp.json`  ->  "exa": { "type": "http", "url": "https://mcp.exa.ai/mcp" }
//
// What that entry IS: a reviewed SYNTAX for a text preview. The URL comes from the publisher's README at the catalog's pinned commit; the `type`/`url` shape comes from
// Claude Code's own documentation of project-scoped `.mcp.json` (docs/config-02a/SOURCES_AND_DECLARATION.md). What it is NOT: proof that the endpoint is the publisher's, is
// reachable, free, private or safe; a connection, an installation, an approval of the tool, or a runtime test. Hosted search can transmit what a user later asks of it and
// may be rate-limited or billed; credentials are never part of this stanza (no key, no token, no query string, no header). The comparison against the sources was made by
// the developer and awaits independent review; the data says so in `review.note`.
//
// Adding or changing an entry is a reviewed DATA change, never a code path: it needs a reviewer id, evidence ids from docs/RESEARCH-SOURCES.md, the pinned source commit the
// declaration was reviewed against (the planner refuses it when the catalog entry's own pinned commit or repository differs), an exact version for any package runner (no
// dist-tags), no literal secret (credentials only as `${NAME}` references, and only where the host documents expansion), and it must pass `validateDeclaration` in
// configPlan.ts. With no entry for a pair the planner answers BLOCKED_INSUFFICIENT_EVIDENCE and produces no diff. Tests inject synthetic registries for everything else.
import type { HostId, JsonValue } from "./configPlan";

export interface ReviewedDeclaration {
  hostId: HostId;
  entryId: string;
  /** The key under `mcpServers`. */
  serverName: string;
  /** The exact stanza for that key. */
  declaration: { [key: string]: JsonValue };
  review: {
    reviewId: string;
    /** Evidence ids registered in docs/RESEARCH-SOURCES.md (R-numbers). */
    evidenceIds: string[];
    /** The full 40-hex commit of the publisher source the declaration was reviewed against. The catalog entry must pin the SAME commit or the declaration is not used. */
    reviewedAgainst: string;
    /** When present, the catalog entry's own repository URL must equal it exactly. */
    repositoryUrl?: string;
    /** Shown with the preview: what kind of review this was and what it did not establish. */
    note?: string;
  };
  /** Plain Turkish sentences shown with the preview (cost, privacy, authentication, source limits). Static data, never built from input. */
  warnings?: string[];
}

export const REVIEWED_DECLARATIONS: readonly ReviewedDeclaration[] = [
  {
    hostId: "claude-code-project-mcp",
    entryId: "exa-mcp-server",
    serverName: "exa",
    // Claude Code documents `"type": "http"` for a remote server in `.mcp.json` (and accepts `"streamable-http"` as an alias); a `url` without a `type` is a configuration error.
    // The publisher's generic example says `streamable-http` and, as published, ends an object with a trailing comma (not valid JSON): it is not copied. No key, token, header,
    // query string or `tools=` parameter: the bare endpoint selects the publisher's default tools.
    declaration: { type: "http", url: "https://mcp.exa.ai/mcp" },
    review: {
      reviewId: "SRC-CONFIG-02A-EXA-1",
      evidenceIds: ["R37", "R55", "R56"],
      reviewedAgainst: "0b499f095c4ca145eee07b09137641c037d58449",
      repositoryUrl: "https://github.com/exa-labs/exa-mcp-server",
      note: "Yalnızca sözdizimi, yalnızca önizleme için gözden geçirildi; çalışma zamanı doğrulanmadı. Kaynak karşılaştırmasını geliştirici yaptı; bağımsız inceleme bekliyor.",
    },
    warnings: [
      "Barındırılan bir arama hizmeti: ileride bağlanırsanız ajanın gönderdiği arama istekleri (metinler, adresler) üçüncü taraf bir hizmete gider. Hizmetin ücret, kullanım sınırı ve mahremiyet koşulları ayrıca değerlendirilmelidir.",
      "Yayıncı belgesi, anahtarsız kullanımın hız sınırlarıyla mümkün olduğunu, daha yüksek sınırlar ve bazı araçlar için OAuth ya da API anahtarı gerektiğini söyler. Bu taslak anahtar, belirteç, başlık ya da sorgu parametresi içermez ve içermemelidir: anahtarın adrese konması yapılandırma dosyalarına ve günlüklere sızar.",
      "Adres ve aktarım türü yayıncının belgesinden (sabit commit) alınmıştır. Bu adresin kimliği, erişilebilirliği ya da güvenliği doğrulanmadı ve bu uygulama ona bağlanmaz.",
      "Taslak “type”: “http” yazar (Claude Code belgesinin birincil adı); yayıncının genel örneği “streamable-http” kullanır ve Claude Code belgesi bunu eşanlamlı sayar. Yayıncının örneği, sondaki virgül nedeniyle geçerli JSON da değildir; olduğu gibi kopyalanmadı.",
    ],
  },
];
