// @ts-expect-error TS2591: no Node type definitions in this project
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Static contract (jsdom does not measure layout). The measured regression is the synthetic browser check ATM_UI_RESPONSIVE_FIX_01_R1.
const css: string = readFileSync("src/styles.css", "utf8");
const block = /@media \(max-width: 760px\) \{([^\n]*)\}\s*\n/.exec(css)![1];

describe("collapsed sidebar at 760 CSS px and below", () => {
  it("the single shell track may shrink below the sidebar's minimum content width", () => {
    expect(block).toContain(".shell { grid-template-columns: minmax(0, 1fr); }");
    expect(block).not.toMatch(/grid-template-columns: 1fr/);
  });
  it("the navigation list wraps instead of forming one non-wrapping row", () => {
    expect(block).toMatch(/\.sidebar ul \{[^}]*display: flex;[^}]*flex-wrap: wrap;/);
    expect(block).not.toContain("grid-auto-flow: column");
  });
  it("the card grid rule is unchanged (its column count follows the width of its parent)", () => {
    expect(css).toContain(".ov-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 20rem), 1fr));");
  });
});
