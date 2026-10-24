// CONFIG-01 - the registry of EXACT MCP declarations that an independent review has approved for a (host file, catalog entry) pair.
//
// It is EMPTY on purpose. Nothing in this repository qualifies yet:
//   * a publisher's install command, a README example, an `npx ...@latest` call or an API key placed in a URL is not an approved declaration;
//   * the hosted endpoints named in the catalog (for example Exa's and Apify's) are informational, not authorised connections;
//   * the Playwright MCP package is documented only with an unpinned `latest` tag, and running it through a package runner fetches code from a registry.
// Adding an entry here is a reviewed DATA change, never a code path: it needs the reviewer's id, evidence ids from docs/RESEARCH-SOURCES.md, the pinned source
// commit the declaration was reviewed against, an exact version (no dist-tags), no literal secret (credentials only as `${NAME}` references, and only where the
// host documents expansion), and it must pass `validateDeclaration` in configPlan.ts. With no entry for a pair the planner answers
// BLOCKED_INSUFFICIENT_EVIDENCE and produces no diff. Tests inject synthetic registries; this constant is what the application ships.
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
    /** The full 40-hex commit of the publisher source the declaration was reviewed against. */
    reviewedAgainst: string;
  };
}

export const REVIEWED_DECLARATIONS: readonly ReviewedDeclaration[] = [];
