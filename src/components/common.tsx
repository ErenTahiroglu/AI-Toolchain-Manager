import { Component, useCallback, useEffect, useState, type ReactNode } from "react";
import { reportError, toAppError } from "../api";
import { statusLabel } from "../labels";
import type { AppError } from "../types";

const ICON: Record<string, string> = { passed: "✓", failed: "✕", unknown: "?", skipped: "–", blocked: "⛔" };

/** Status is always conveyed by text AND symbol, never by colour alone. */
export function StatusBadge({ status }: { status: string }) {
  return (
    <span className={`badge badge-${status}`}>
      <span aria-hidden="true">{ICON[status] ?? "•"}</span> {statusLabel(status)}
    </span>
  );
}

export function SyntheticBanner({ children }: { children: ReactNode }) {
  return (
    <div className="banner banner-synthetic" role="note">
      <strong>ÖRNEK VERİ</strong> — {children}
    </div>
  );
}

export function ErrorNotice({ error, onRetry }: { error: AppError; onRetry?: () => void }) {
  return (
    <div className="banner banner-error" role="alert">
      <strong>Bir sorun oluştu.</strong> {error.message} <code>{error.code}</code>
      {onRetry && (
        <>
          {" "}
          <button type="button" onClick={onRetry}>Yeniden dene</button>
        </>
      )}
    </div>
  );
}

export function UnavailableButton({ label, reason }: { label: string; reason: string }) {
  return (
    <span className="unavailable">
      <button type="button" disabled aria-disabled="true" title={reason}>{label}</button>
      <span className="muted small"> {reason}</span>
    </span>
  );
}

export function useLoad<T>(loader: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(true);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const load = useCallback(() => {
    setLoading(true);
    loader()
      .then((d) => { setData(d); setError(null); })
      .catch((e) => setError(toAppError(e)))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  useEffect(load, [load]);
  return { data, error, loading, reload: load, setData };
}

interface BoundaryState { error: Error | null }

/** Last line of defence: a render crash shows a readable message and is written to the local log. */
export class ErrorBoundary extends Component<{ children: ReactNode }, BoundaryState> {
  state: BoundaryState = { error: null };
  static getDerivedStateFromError(error: Error): BoundaryState {
    return { error };
  }
  componentDidCatch(error: Error) {
    reportError(error);
  }
  render() {
    if (this.state.error) {
      return (
        <div className="banner banner-error" role="alert">
          <h2>Ekranda beklenmeyen bir hata oluştu</h2>
          <p>Hata yerel tanılama kaydına yazıldı (kişisel yollar ve gizli değerler silinerek). Hiçbir veri dışarı gönderilmedi.</p>
          <button type="button" onClick={() => this.setState({ error: null })}>Ekrana dön</button>
        </div>
      );
    }
    return this.props.children;
  }
}
