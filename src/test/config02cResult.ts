// CONFIG-02C: the same machine-visible result line the Rust cases print (`CONFIG02C_RESULT|<id>|<os>|<STATUS>|<reason>`), for the cases that run in vitest/jsdom.
// jsdom results are NOT evidence of real-window behaviour (keyboard focus, screen reader, WebView2 egress): those stay NOT RUN. Each wrapped case prints exactly one line.
import { it } from "vitest";

const platform = (globalThis as unknown as { process?: { platform?: string } }).process?.platform;
export const OS = platform === "win32" ? "windows" : platform === "darwin" ? "macos" : platform ?? "unknown";

const line = (id: string, status: "PASS" | "FAIL" | "SKIPPED", reason = "") => console.log(`CONFIG02C_RESULT|${id}|${OS}|${status}|${reason}`);

/** An `it` that prints one result line: PASS when the body finished, FAIL (and the original error) when it threw. */
export function caseIt(id: string, name: string, body: () => void | Promise<void>): void {
  it(`${id} - ${name}`, async () => {
    try {
      await body();
    } catch (e) {
      line(id, "FAIL", "assertion");
      throw e;
    }
    line(id, "PASS");
  });
}
