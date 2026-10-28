// CONFIG-02B - the TypeScript preview and the Rust transaction PROTOTYPE (src-tauri/src/config_txn.rs, tests only) must produce the SAME
// "after" text for the same file: the preview is what the user is shown, the writer is what a future real phase would write. The cases
// live in one data file that both test suites read. Pure and deterministic: nothing is read from disk at run time but that bundled file.
import { describe, expect, it } from "vitest";
import golden from "../tests/fixtures/config-02b-golden.json";
import { REVIEWED_DECLARATIONS } from "./configDeclarations";
import { planConfigPreview, type PlanRequest } from "./configPlan";
import { exaEntry } from "./test/exaEntry";

interface Case {
  name: string;
  input: string | null;
  expectedAfter: string;
}
const cases = golden.cases as Case[];

const request = (input: string | null): PlanRequest => ({
  alias: "benim-sitem",
  platform: "windows-x64",
  surface: "claude-code-terminal",
  entry: exaEntry(),
  source: input === null ? { kind: "new" } : { kind: "text", text: input },
});

describe("CONFIG-02B golden cases: the preview's 'after' text is the text the Rust prototype writes", () => {
  it("has the cases the Rust side also reads", () => {
    expect(cases.length).toBeGreaterThanOrEqual(5);
    expect(cases[0].input).toBeNull();
  });

  for (const c of cases) {
    it(`preview text equals the golden 'after': ${c.name}`, () => {
      const plan = planConfigPreview(request(c.input));
      expect(plan.status).toBe("PREVIEW_ONLY_NOT_APPLIED");
      expect(plan.maskedValues).toBe(0); // nothing secret-looking in the fixtures, so the redacted preview IS the full text
      expect(plan.proposed).toBe(c.expectedAfter);
    });
  }

  it("the golden stanza is the shipped reviewed declaration", () => {
    const [d] = REVIEWED_DECLARATIONS;
    const stanza = JSON.parse(cases[0].expectedAfter).mcpServers[d.serverName];
    expect(stanza).toEqual(d.declaration);
  });
});
