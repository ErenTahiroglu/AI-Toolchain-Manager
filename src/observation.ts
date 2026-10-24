// PRODUCT-04 - what the Project Tool Workspace may say about the last STORED discovery observation of one tool in one target.
//
// Pure. The only input is the answer of the existing, typed, read-only `list_inventory` (stored database rows; it scans nothing) and the matrix row.
// The backend already classified every ACCEPTED wish against its stored observations (`Inventory.requests`, store.rs::classify_request); this module
// only finds the one backend row that belongs to the row on screen - by ALL FOUR key fields, never by tool id, display name or source name alone -
// and refuses to say more than the backend said. It never turns "found by name" into "installed", "working" or "verified", and it never turns a
// failed or missing read into "not scanned" / "not found".
import type { AppError, Inventory, RequestStatus, RequestVsObservation } from "./types";
import type { WorkspaceRow } from "./workspace";

export const REQUEST_STATUSES: readonly RequestStatus[] = [
  "present_current", "absent_current", "unknown_current",
  "previously_present", "previously_absent", "previously_unknown",
  "platform_mismatch", "not_scanned",
];
export const isKnownRequestStatus = (s: string): s is RequestStatus => (REQUEST_STATUSES as readonly string[]).includes(s);

/** The matrix's view of the `list_inventory` load (the shape `useLoad` returns). */
export interface InventoryRead {
  data: Inventory | null;
  error: AppError | null;
  loading: boolean;
}

export interface ObservationTarget {
  alias: string;
  platform: string;
  surface: string;
}

export type ObservationView =
  /** No inventory answer yet. Nothing is shown as an observation. */
  | { kind: "loading" }
  /** The first read failed. This is NOT "not scanned" and NOT "not found". */
  | { kind: "unreadable"; error: AppError }
  /** The catalog entry was removed: no comparison is made and no current observation is invented. */
  | { kind: "removed" }
  /** No ACCEPTED wish for this exact target: the backend compares accepted wishes only, so there is nothing to compare. */
  | { kind: "no_wish" }
  /** An accepted wish, but the inventory holds no row for this exact key (for example it was read before the wish was saved). */
  | { kind: "no_row"; reading: boolean; old: boolean }
  /** More than one backend row for the same key: contradictory, so none is shown. */
  | { kind: "ambiguous" }
  /** A status this version does not know. It is never treated as a success. */
  | { kind: "unrecognised"; status: string; request: RequestVsObservation; old: boolean }
  /** The backend's classification, unchanged. `old`: a later refresh failed, so this comes from an earlier successful read. */
  | { kind: "observed"; status: RequestStatus; request: RequestVsObservation; old: boolean };

/** The exact key: project + tool + operating system + agent surface. All four, or no match. */
export const requestMatches = (r: RequestVsObservation, entryId: string, t: ObservationTarget): boolean =>
  r.projectAlias === t.alias && r.entryId === entryId && r.platformTarget === t.platform && r.agentSurface === t.surface;

export function observationFor(row: WorkspaceRow, target: ObservationTarget, inventory: InventoryRead): ObservationView {
  // these two answers do not depend on the inventory at all
  if (row.removed) return { kind: "removed" };
  if (row.pref?.decision !== "accepted") return { kind: "no_wish" };

  const data = inventory.data;
  if (!data) return inventory.error ? { kind: "unreadable", error: inventory.error } : { kind: "loading" };

  const old = inventory.error !== null; // a refresh failed: what we hold is the earlier successful read
  const hits = data.requests.filter((r) => requestMatches(r, row.entry.entryId, target));
  if (hits.length === 0) return { kind: "no_row", reading: inventory.loading, old };
  if (hits.length > 1) return { kind: "ambiguous" };
  const request = hits[0];
  return isKnownRequestStatus(request.status) ? { kind: "observed", status: request.status, request, old } : { kind: "unrecognised", status: request.status, request, old };
}
