// CONFIG-02C test data: one valid backend answer per status (and one per reason), exactly in the contract's shape. Made up; no real file or path.
const NOT_CLAIMS = ["not_loaded", "not_trusted", "not_installed", "not_enabled", "not_reachable", "not_verified"];
const base = {
  schema: "config02c.read.v1", reason: null as string | null, alias: "benim-sitem", file: ".mcp.json", scope: "project", surface: "claude-code-terminal", maxBytes: 1048576,
  sizeBytes: null as number | null, sha256: null as string | null, serverCount: null as number | null, reviewedDeclarations: null as unknown, notClaims: NOT_CLAIMS,
};
const bytes = { sizeBytes: 51, sha256: "cd".repeat(32) };

export const ALL_STATUS_SAMPLES: (typeof base & { status: string })[] = [
  { ...base, status: "root_not_found" },
  { ...base, status: "absent" },
  { ...base, status: "present_summarized", ...bytes, serverCount: 3, reviewedDeclarations: [{ id: "exa-mcp-server", nameState: "absent", endpointState: "absent" }] },
  { ...base, status: "present_summarized", ...bytes, serverCount: 0, reviewedDeclarations: [{ id: "exa-mcp-server", nameState: "present_other_letter_case", endpointState: "present_under_other_name" }] },
  { ...base, status: "present_summarized", ...bytes, serverCount: 1, reviewedDeclarations: [{ id: "exa-mcp-server", nameState: "present", endpointState: "not_evaluable" }] },
  ...["duplicate_key", "byte_order_mark", "root_not_object", "servers_not_object", "too_deep", "too_many_servers"].map((reason) => ({ ...base, status: "present_metadata_only", reason, ...bytes })),
  ...["invalid_utf8", "invalid_json", "lone_surrogate", "empty_file"].map((reason) => ({ ...base, status: "malformed", reason, ...bytes })),
  ...["access_denied", "locked"].map((reason) => ({ ...base, status: "denied", reason })),
  ...["link", "not_a_directory", "not_regular_file", "hard_linked", "case_alias", "invalid_root_text", "root_too_broad"].map((reason) => ({ ...base, status: "unsafe_path", reason })),
  { ...base, status: "unsupported_fs" },
  { ...base, status: "too_large" },
  { ...base, status: "changed_during_read" },
  { ...base, status: "io_error" },
];
