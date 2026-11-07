// ATM-GOV-02 P2C-WP2 - the Checkpoint Merkezi page: the project overview (cards), the manual paste viewer (ATM-GOV-01, untouched) and the one ECE panel (P2B).
// This component owns the SESSION-ONLY state: the claims the user typed (a Map in React state: no storage, no file, no command) and the ECE observation slot.
// It makes no native call of its own except the existing read-only list of registered projects. The ECE panel is the only producer of observations; the
// manual paste viewer is not connected to it. Leaving the page unmounts this component, so claims and observation vanish.
import { useCallback, useRef, useState } from "react";
import { api } from "../api";
import { ErrorNotice, useLoad } from "../components/common";
import { CiEvidencePanel } from "../components/CiEvidencePanel";
import { ProjectCheckpointOverview } from "../components/ProjectCheckpointOverview";
import { ATM_ROW_KEY, type EceObservation, type ObservationEvent } from "../ciAssessment";
import { isAtmRepository } from "../claimsForm";
import { buildRows, type ObservationSlot, type UserClaims } from "../projectCheckpoint";
import { CheckpointsPage } from "./Checkpoints";

/** The key the ECE observation binds to: the oldest real project claiming the ATM repository, else the fixed virtual row. Same rule as `buildRows`. */
function atmKeyOf(projects: { projectId: string; kind: string; createdAt: string }[], claims: ReadonlyMap<string, UserClaims>): string {
  const mine = projects
    .filter((p) => p.kind === "reference" && isAtmRepository(claims.get(p.projectId)?.repository ?? null))
    .sort((a, b) => (a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 : a.projectId < b.projectId ? -1 : 1));
  return mine[0]?.projectId ?? ATM_ROW_KEY;
}

export function CheckpointCenter() {
  const projects = useLoad(api.listProjects);
  const [claims, setClaims] = useState<ReadonlyMap<string, UserClaims>>(new Map());
  const [slot, setSlot] = useState<ObservationSlot | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const pendingKey = useRef<string | null>(null); // the ATM row key at the moment the running submission started

  // `list` only feeds the panel key (so a failed reload does not reset the form); the SUMMARY is built only from a list that was read successfully (`ready`)
  const list = projects.data ?? [];
  const ready = projects.data !== null && projects.error === null;
  const atmKey = atmKeyOf(list, claims);
  const atmKeyRef = useRef(atmKey);
  atmKeyRef.current = atmKey;

  const onObservation = useCallback((event: ObservationEvent) => {
    if (event.kind === "started") {
      pendingKey.current = atmKeyRef.current; // remember WHICH row this submission belongs to; an older observation never survives a new submission
      setSlot(null);
    } else if (event.kind === "cleared") {
      pendingKey.current = null;
      setSlot(null);
    } else {
      const bound = pendingKey.current;
      pendingKey.current = null;
      // an answer whose row has changed since the submission started (or that has no matching start) is dropped
      if (bound !== null && bound === atmKeyRef.current) setSlot({ obs: event.obs as EceObservation, boundRowKey: bound });
    }
  }, []);

  const save = (projectId: string, next: UserClaims): string | null => {
    if (isAtmRepository(next.repository)) {
      const other = list.find((p) => p.kind === "reference" && p.projectId !== projectId && isAtmRepository(claims.get(p.projectId)?.repository ?? null));
      if (other) return `ATM deposu zaten “${other.displayName}” projesinde beyan edilmiş; aynı depo iki canlı projede beyan edilemez.`;
    }
    setClaims(new Map(claims).set(projectId, next));
    return null;
  };
  const clear = (projectId: string) => {
    const next = new Map(claims);
    next.delete(projectId);
    setClaims(next);
  };
  const goToPanel = () => panelRef.current?.focus();

  const rows = buildRows(list, claims, slot);

  return (
    <>
      {projects.error && (
        <section className="ov" aria-label="Proje checkpoint özeti yüklenemedi">
          <h2>Proje Checkpoint Özeti</h2>
          <p className="small">Kayıtlı proje listesi okunamadığı için özet oluşturulmadı; hiçbir proje ya da kart gösterilmiyor. Aşağıdaki yapıştırma görüntüleyicisi ve ECE denetim formu bundan etkilenmez.</p>
          <ErrorNotice error={projects.error} onRetry={projects.reload} />
        </section>
      )}
      {!projects.error && projects.data === null && (
        <section className="ov" aria-label="Proje checkpoint özeti yükleniyor">
          <h2>Proje Checkpoint Özeti</h2>
          <p className="muted" aria-live="polite">Proje listesi yükleniyor…</p>
        </section>
      )}
      {ready && <ProjectCheckpointOverview rows={rows} hiddenSynthetic={list.filter((p) => p.kind !== "reference").length} onSave={save} onClear={clear} onGoToPanel={goToPanel} />}
      <CheckpointsPage />
      {/* keyed by the ATM row: when the row the panel belongs to changes, the panel (its inputs and any pending answer) is dropped with it */}
      <div ref={panelRef} tabIndex={-1} className="ov-panel-anchor">
        <CiEvidencePanel key={atmKey} onObservation={onObservation} />
      </div>
    </>
  );
}
