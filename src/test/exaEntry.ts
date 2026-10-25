// CONFIG-02A - the REAL catalog record of `exa-mcp-server`, read from the data file the application seeds from (not a synthetic stand-in), shaped like the entry the backend
// returns. Test helper only; nothing here is shipped.
import candidates from "../../data/catalog-candidates-c01.json?raw";
import { makeEntry } from "./backend";
import type { CatalogEntry } from "../types";

interface Wrapper {
  entryKind: string;
  executionPolicy: CatalogEntry["executionPolicy"];
  purpose: string;
  record: CatalogEntry["record"] & { toolId: string; displayName: string; reviewState: string; repository: { url: string }; sourceSnapshot: { immutableCommit: string } };
}

export const EXA_WRAPPER: Wrapper = (JSON.parse(candidates) as { records: Wrapper[] }).records.find((r) => r.record.toolId === "exa-mcp-server")!;

/** The Exa entry exactly as the catalog data describes it, with the usual overrides for negative tests. */
export const exaEntry = (over: Partial<CatalogEntry> = {}): CatalogEntry =>
  makeEntry("exa-mcp-server", {
    displayName: EXA_WRAPPER.record.displayName,
    kind: "agent_plugin",
    reviewState: EXA_WRAPPER.record.reviewState,
    executionPolicy: EXA_WRAPPER.executionPolicy,
    purpose: EXA_WRAPPER.purpose,
    repoUrl: EXA_WRAPPER.record.repository.url,
    record: JSON.parse(JSON.stringify(EXA_WRAPPER.record)),
    ...over,
  });
