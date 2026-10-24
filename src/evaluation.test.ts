// PRODUCT-03 - the pure logic that binds an evaluation to the snapshot of its inputs. No rendering, no backend.
import { describe, expect, it } from "vitest";
import { FRONTEND_ENTRY, answerProblem, changedSince, parseEvidenceFiles, toTaskInput, type EvalSnapshot } from "./evaluation";
import type { Evaluation } from "./types";

const snap = (over: Partial<EvalSnapshot> = {}): EvalSnapshot => ({
  alias: "a", platform: "windows-x64", surface: "claude-code-terminal", taskId: "gorev-1", taskText: "React bileşeni", evidenceFiles: ["A.tsx"], phase: "start", frontendScope: "none", ...over,
});
const answer = (over: Partial<Evaluation> = {}): Evaluation => ({ taskId: "gorev-1", phase: "start", recommendations: [], suppressed: [], disclaimer: "x", ...over });

describe("parseEvidenceFiles", () => {
  it("keeps only trimmed, non-empty names, one per line, and never rewrites a name", () => {
    expect(parseEvidenceFiles("  A.tsx \n\n B.css\n   \n")).toEqual(["A.tsx", "B.css"]);
    expect(parseEvidenceFiles("")).toEqual([]);
    expect(parseEvidenceFiles("src/İç Dosya.tsx")).toEqual(["src/İç Dosya.tsx"]);
  });
});

describe("toTaskInput: the same wire fields as before, the text exactly as typed", () => {
  it("maps every field and sends no availability unless a scope was assumed", () => {
    expect(toTaskInput(snap({ taskText: "  boşluklu  metin \n" }))).toEqual({
      projectAlias: "a", taskId: "gorev-1", taskText: "  boşluklu  metin \n", evidenceFiles: ["A.tsx"], agentSurface: "claude-code-terminal", platformTarget: "windows-x64", phase: "start", availability: [],
    });
  });
  it("a hypothetical scope becomes exactly one availability entry for the frontend plugin", () => {
    expect(toTaskInput(snap({ frontendScope: "project", phase: "late" }))).toMatchObject({ phase: "late", availability: [{ entryId: FRONTEND_ENTRY, scope: "project", enabled: true }] });
  });
});

describe("answerProblem: an answer must be for the question that was asked", () => {
  it("accepts a matching answer", () => expect(answerProblem(snap(), answer())).toBeNull());
  it("refuses another task id, another phase, a malformed answer and a recommendation without an entry id", () => {
    expect(answerProblem(snap(), answer({ taskId: "gorev-2" }))).toMatch(/başka bir göreve/);
    expect(answerProblem(snap(), answer({ phase: "late" }))).toMatch(/aşaması/);
    expect(answerProblem(snap(), null as unknown as Evaluation)).toMatch(/biçimi/);
    expect(answerProblem(snap(), { ...answer(), recommendations: undefined } as unknown as Evaluation)).toMatch(/biçimi/);
    expect(answerProblem(snap(), answer({ recommendations: [{ entryId: "", displayName: "x", level: "suggest", canEnable: true, rationale: "", impact: "", signals: [], notes: [], skillCandidates: [] }] }))).toMatch(/kimliği eksik/);
  });
});

describe("changedSince names what the user changed", () => {
  it("empty when nothing but the task id differs", () => expect(changedSince(snap(), snap({ taskId: "başka" }))).toEqual([]));
  it("lists each changed input in words", () => {
    expect(changedSince(snap(), snap({ alias: "b", platform: "macos-arm64", surface: "local-cli", taskText: "y", evidenceFiles: [], phase: "late", frontendScope: "global" }))).toEqual([
      "proje", "işletim sistemi", "ajan yüzeyi", "görev metni", "dosya adları", "görev aşaması", "varsayımsal kurulum durumu",
    ]);
  });
});
