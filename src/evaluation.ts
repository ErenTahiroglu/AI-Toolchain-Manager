// PRODUCT-03 - pure logic of one task evaluation on the Recommendations page. No I/O, no React, no clock.
// An evaluation is bound to a SNAPSHOT of every input it was started with. A result, an error or a "loading" flag belongs to exactly one snapshot
// and may only be shown while the form still describes that snapshot; the page never lets an answer for one context appear under another.
import type { Evaluation, TaskInput } from "./types";

/** The hypothetical "Frontend Design is already installed" answer the user gives. It is an assumption, never an observation. */
export type FrontendScope = "none" | "global" | "project";
export const FRONTEND_ENTRY = "anthropic-frontend-design";

export interface EvalSnapshot {
  alias: string;
  platform: string;
  surface: string;
  taskId: string;
  taskText: string;
  /** File NAMES only, trimmed, empty lines dropped - exactly what is sent. The application never opens them. */
  evidenceFiles: string[];
  phase: "start" | "late";
  frontendScope: FrontendScope;
}

export const parseEvidenceFiles = (raw: string): string[] => raw.split("\n").map((f) => f.trim()).filter(Boolean);

/** The wire input of evaluate_task for a snapshot. Same fields and meaning as before PRODUCT-03; the text is sent as typed. */
export function toTaskInput(s: EvalSnapshot): TaskInput {
  return {
    projectAlias: s.alias,
    taskId: s.taskId,
    taskText: s.taskText,
    evidenceFiles: s.evidenceFiles,
    agentSurface: s.surface,
    platformTarget: s.platform,
    phase: s.phase,
    availability: s.frontendScope === "none" ? [] : [{ entryId: FRONTEND_ENTRY, scope: s.frontendScope, enabled: true }],
  };
}

/**
 * The answer must describe the task that was asked about. Returns a reason when it does not (a different task id or phase, or a
 * recommendation without an entry id) so that it is refused instead of being shown as the answer for this snapshot.
 */
export function answerProblem(s: EvalSnapshot, e: Evaluation): string | null {
  if (e === null || typeof e !== "object" || !Array.isArray(e.recommendations) || !Array.isArray(e.suppressed)) return "Yanıtın biçimi tanınmadı.";
  if (e.taskId !== s.taskId) return "Yanıt başka bir göreve ait görünüyor.";
  if (e.phase !== s.phase) return "Yanıtın aşaması istenen aşamayla eşleşmedi.";
  if (e.recommendations.some((r) => typeof r?.entryId !== "string" || r.entryId === "")) return "Yanıttaki bir önerinin araç kimliği eksik.";
  return null;
}

/** What differs between the form now and the snapshot a result was made for, in words. Empty when the result is still current. */
export function changedSince(result: EvalSnapshot, now: EvalSnapshot): string[] {
  const out: string[] = [];
  if (result.alias !== now.alias) out.push("proje");
  if (result.platform !== now.platform) out.push("işletim sistemi");
  if (result.surface !== now.surface) out.push("ajan yüzeyi");
  if (result.taskText !== now.taskText) out.push("görev metni");
  if (JSON.stringify(result.evidenceFiles) !== JSON.stringify(now.evidenceFiles)) out.push("dosya adları");
  if (result.phase !== now.phase) out.push("görev aşaması");
  if (result.frontendScope !== now.frontendScope) out.push("varsayımsal kurulum durumu");
  return out;
}
