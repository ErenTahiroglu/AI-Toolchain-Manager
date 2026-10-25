// PRODUCT-07 H - the read-only "Kapsama göre MCP tanımı" block under a stored presence result (matrix) or a review-queue item. It renders
// src/scopedDeclarations.ts and nothing else: it loads nothing, starts no scan, has no button, creates no review reason and changes no count.
// Deliberately neutral: no success/danger colour, no badge, and no word for "conflict" - a program found by name and a project file without a matching
// definition are two different answers that can both be true.
import { useId } from "react";
import { surfaceLabel } from "../labels";
import {
  describeScopedDeclarations, notImplementedExtra, NOT_IMPLEMENTED_TEXT, SCOPE_NAME, scopeLines, SCOPED_DISCLAIMER, SCOPED_HEADING, SCOPED_NOT_READ_NOTE, SCOPED_SEPARATE,
} from "../scopedDeclarations";

export function ScopedConfigNote({ request }: { request: { agentSurface: string; declarationDetector?: unknown; declarations?: unknown } }) {
  const uid = useId();
  const view = describeScopedDeclarations(request);
  if (view.kind === "unavailable") return null; // an older backend, or a payload that is not believed: no block and no coverage claim
  return (
    <section className="ws-scoped small" aria-labelledby={`${uid}-h`} data-testid="scoped-config">
      <h5 id={`${uid}-h`}>{SCOPED_HEADING}</h5>
      {view.kind === "not_implemented" ? (
        <>
          <p className="muted">{NOT_IMPLEMENTED_TEXT}</p>
          <p className="muted">{notImplementedExtra(view.surface)} <span className="sr-only">({surfaceLabel(view.surface)})</span></p>
        </>
      ) : (
        <>
          <p className="muted">{SCOPED_SEPARATE}</p>
          <ul aria-label="Kapsamlara göre MCP tanımı okumaları">
            {scopeLines(view).map((l) => (
              <li key={l.scope} data-scope={l.scope}>
                <strong>{SCOPE_NAME[l.scope]}:</strong> {l.text}
              </li>
            ))}
          </ul>
          <p className="muted">{SCOPED_NOT_READ_NOTE}</p>
          <p className="muted">{SCOPED_DISCLAIMER}</p>
        </>
      )}
    </section>
  );
}
