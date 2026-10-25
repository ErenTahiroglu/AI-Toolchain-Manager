// PRODUCT-06 - the read-only "Kanıtın kaynağı" lines under a stored observation (matrix) or a review-queue item. It renders src/origin.ts and nothing
// else: it loads nothing, starts no scan and has no button. No path, location, raw evidence, fingerprint or version is ever shown here.
import { describeOrigin, ORIGIN_HEADING, ORIGIN_NO_OBSERVATION, ORIGIN_SEPARATE, ORIGIN_UNAVAILABLE, originMethodText, originTimeText, PROJECT_TEXT, SURFACE_TEXT } from "../origin";

export function EvidenceOriginNote({ origin, hasBasis }: { origin: unknown; hasBasis: boolean }) {
  const view = describeOrigin(origin);
  const time = view.kind === "described" ? originTimeText(view) : null;
  const method = view.kind === "described" ? originMethodText(view) : null;
  return (
    <div className="ws-origin small" data-testid="evidence-origin">
      <p>
        <strong>{ORIGIN_HEADING}</strong> <span className="muted">{ORIGIN_SEPARATE}</span>
      </p>
      {view.kind === "described" ? (
        <ul className="muted">
          <li>{PROJECT_TEXT[view.project]}</li>
          <li>{SURFACE_TEXT[view.surface]}</li>
          {method && <li>{method}</li>}
          {time && <li>{time}</li>}
        </ul>
      ) : (
        <p className="muted">
          {ORIGIN_UNAVAILABLE}
          {!hasBasis && ` ${ORIGIN_NO_OBSERVATION}`}
        </p>
      )}
    </div>
  );
}
