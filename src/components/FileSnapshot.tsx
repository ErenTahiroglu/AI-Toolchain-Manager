// CONFIG-02C (Issue #40) - "Dosyadan oku (isteğe bağlı)": ONE explicit, user-confirmed, read-only look at `.mcp.json` directly inside ONE folder the user types for this click.
// Collapsed and off by default. One click on the read button = one backend call = one closed typed answer, held in this component's memory only. Declining (or never opening
// the section) makes no backend call at all. Nothing is stored, copied, exported, linked or logged; there is no apply control; the pasted-text preview next to it is unaffected.
// The typed folder is in the window's memory while it is typed and is removed from this component's state when the call returns - that is hygiene, not a secure erase (#25).
// Everything shown comes from fixed tables in src/fileSnapshot.ts; the raw answer is accepted only when it has exactly the contract's closed shape.
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { api, toAppError } from "../api";
import { sha256Hex } from "../configPlan";
import {
  CLEAR_BUTTON, DECLINED_TEXT, DECLINE_BUTTON, FOLDER_HELP, FOLDER_LABEL, FOLDER_PLACEHOLDER, FOOTER, GATE_LEAD, GATE_POINTS, HOST_NOTE, NOT_REQUESTED_TEXT, READING_TEXT,
  READ_BUTTON, READ_CONSENT, SECTION_TITLE, SNAPSHOT_LABEL, SNAPSHOT_TTL_MS, UNEXPECTED_TEXT, describeResult, errorText, pasteBridge, type SnapshotView,
} from "../fileSnapshot";

type Phase =
  | { kind: "idle" }
  | { kind: "reading" }
  | { kind: "done"; view: SnapshotView; clock: string }
  | { kind: "unexpected" }
  | { kind: "error"; text: string };

const clock = () => new Date().toLocaleTimeString("tr-TR");

export function FileSnapshot({ alias, projectName, platform, surface, entryId, pastedText }: {
  alias: string;
  projectName: string;
  platform: string;
  surface: string;
  entryId: string;
  /** The text the user pasted into the preview, if any; used only to compare digests at display time. */
  pastedText: string | null;
}) {
  const uid = useId();
  const [open, setOpen] = useState(false);
  const [directory, setDirectory] = useState("");
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [declined, setDeclined] = useState(false);
  const generation = useRef(0); // an answer that arrives after a clear, a context change or an unmount is dropped
  const resultRef = useRef<HTMLElement>(null);

  const clearAll = () => {
    generation.current += 1;
    setDirectory("");
    setPhase({ kind: "idle" });
  };

  // a change of project, operating system, surface or catalog entry clears everything
  const context = `${alias}|${platform}|${surface}|${entryId}`;
  const lastContext = useRef(context);
  useEffect(() => {
    if (lastContext.current !== context) {
      lastContext.current = context;
      clearAll();
      setDeclined(false);
    }
  }, [context]);

  // leaving the page (unmount) invalidates any call still in flight
  useEffect(() => () => {
    generation.current += 1;
  }, []);

  // the result expires after five minutes
  useEffect(() => {
    if (phase.kind === "idle" || phase.kind === "reading") return undefined;
    const timer = setTimeout(clearAll, SNAPSHOT_TTL_MS);
    return () => clearTimeout(timer);
  }, [phase]);

  useEffect(() => {
    if (phase.kind === "done") resultRef.current?.focus();
  }, [phase]);

  const read = async () => {
    if (directory.trim() === "" || phase.kind === "reading") return;
    const typed = directory;
    const mine = ++generation.current;
    setDeclined(false);
    setPhase({ kind: "reading" });
    try {
      const raw = await api.readProjectMcpConfig({ alias, directory: typed, consent: READ_CONSENT });
      if (generation.current !== mine) return;
      const view = describeResult(raw, alias);
      setPhase(view ? { kind: "done", view, clock: clock() } : { kind: "unexpected" });
    } catch (e) {
      if (generation.current !== mine) return;
      setPhase({ kind: "error", text: errorText(toAppError(e).code) });
    } finally {
      if (generation.current === mine) setDirectory("");
    }
  };

  // closing the section removes the typed folder and any result from memory; opening it starts from the same empty state every time
  const toggle = () => {
    if (open) clearAll();
    setDeclined(false);
    setOpen(!open);
  };

  const decline = () => {
    clearAll();
    setDeclined(true);
    setOpen(false);
  };

  // computed at display time, never stored
  const bridge = useMemo(
    () => (phase.kind === "done" ? pasteBridge(pastedText !== null && pastedText !== "" ? sha256Hex(pastedText) : null, phase.view.sha256) : null),
    [phase, pastedText],
  );

  const reading = phase.kind === "reading";
  return (
    <div className="cfg-file">
      <details open={open}>
        <summary onClick={(e) => { e.preventDefault(); toggle(); }}>{SECTION_TITLE}</summary>
        {open && (<>
        <div className="small" role="group" aria-labelledby={`${uid}-gate`}>
          <p id={`${uid}-gate`}><strong>{GATE_LEAD}</strong></p>
          <ul>
            {GATE_POINTS.map((p) => (
              <li key={p.lead + p.text.slice(0, 24)}>{p.lead !== "" && <strong>{p.lead}</strong>} {p.text}</li>
            ))}
          </ul>
          <p className="muted">{HOST_NOTE}</p>
        </div>
        <div className="field">
          <label htmlFor={`${uid}-dir`}>{FOLDER_LABEL}</label>
          <input
            id={`${uid}-dir`}
            type="text"
            autoComplete="off"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            value={directory}
            placeholder={FOLDER_PLACEHOLDER}
            disabled={reading}
            aria-describedby={`${uid}-help`}
            onChange={(e) => setDirectory(e.target.value)}
          />
          <div id={`${uid}-help`} className="small muted">{FOLDER_HELP}</div>
        </div>
        <p className="actions">
          <button type="button" className="secondary" onClick={() => void read()} disabled={directory.trim() === "" || reading}>
            {READ_BUTTON}<span className="sr-only"> ({projectName}; hiçbir dosyaya yazmaz)</span>
          </button>{" "}
          <button type="button" className="secondary" onClick={decline} disabled={reading}>{DECLINE_BUTTON}</button>
        </p>
        <div aria-live="polite">
          {phase.kind === "idle" && <p className="small muted">{NOT_REQUESTED_TEXT}</p>}
          {reading && <p role="status" className="small">{READING_TEXT}</p>}
          {phase.kind === "unexpected" && <p role="status" className="small bad-text">{UNEXPECTED_TEXT}</p>}
          {phase.kind === "error" && <p role="status" className="small bad-text">{phase.text}</p>}
        </div>
        {phase.kind === "done" && (
          <section ref={resultRef} tabIndex={-1} role="region" aria-labelledby={`${uid}-h`} className="cfg-result">
            <h4 id={`${uid}-h`}>{SNAPSHOT_LABEL} — {phase.clock}</h4>
            <p role="status"><strong>{phase.view.headline}</strong></p>
            {phase.view.reasonLine !== null && <p className="small">{phase.view.reasonLine}</p>}
            {(phase.view.sizeBytes !== null || phase.view.serverCount !== null) && (
              <dl className="facts small" aria-label="Dosya anlık görüntüsü">
                {phase.view.sizeBytes !== null && (<><dt>Boyut</dt><dd>{phase.view.sizeBytes} bayt</dd></>)}
                {phase.view.sha256 !== null && (<><dt>SHA-256 özeti</dt><dd><code>{phase.view.sha256.slice(0, 16)}…</code> <span className="muted">(okunan baytların özeti)</span></dd></>)}
                {phase.view.serverCount !== null && (<><dt>Sunucu tanımı sayısı</dt><dd>{phase.view.serverCount}</dd></>)}
              </dl>
            )}
            {phase.view.declarationLines.length > 0 && (
              <ul className="small" aria-label="Onaylı Exa bildirimiyle karşılaştırma">{phase.view.declarationLines.map((l) => <li key={l}>{l}</li>)}</ul>
            )}
            {bridge !== null && <p className="small" role="note">{bridge}</p>}
            <p className="small muted">{FOOTER}</p>
            <p className="actions"><button type="button" className="secondary" onClick={clearAll}>{CLEAR_BUTTON}</button></p>
          </section>
        )}
        </>)}
      </details>
      {declined && <p role="status" className="small muted">{DECLINED_TEXT}</p>}
    </div>
  );
}
