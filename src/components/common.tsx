import { Component, useCallback, useEffect, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";
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

/**
 * Loads data through `loader` on mount, whenever `deps` change, and whenever `reload()` is called.
 *
 * Latest request wins: every load takes a generation number, and only the load that holds the CURRENT generation may change state. A slower
 * earlier response (success or failure) and its `loading` bookkeeping are ignored, so an out-of-order read can never put old data - for
 * example an old saved preference - back on the screen. Unmounting (or changing `deps`) invalidates whatever is still in flight, and
 * `reload()` after unmount does nothing. The loader is read through a ref, so `reload()` always uses the newest loader and never a stale
 * closure. `setData(...)` is a newer local value than any read still in flight, so it invalidates that read (and ends its `loading`): the
 * read can no longer overwrite the value that was just set. There is no cache and no automatic retry.
 */
export function useLoad<T>(loader: () => Promise<T>, deps: unknown[] = []) {
  const [data, setRawData] = useState<T | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(true);
  const loaderRef = useRef(loader);
  const generation = useRef(0);
  const alive = useRef(false);
  // declared before the load effect, so a render that changes the loader also changes what the load below calls
  useEffect(() => {
    loaderRef.current = loader;
  });
  const load = useCallback(() => {
    if (!alive.current) return;
    const mine = ++generation.current;
    setLoading(true);
    let pending: Promise<T>;
    try {
      pending = Promise.resolve(loaderRef.current());
    } catch (e) {
      pending = Promise.reject(e);
    }
    pending.then(
      (d) => {
        if (mine !== generation.current) return;
        setRawData(d);
        setError(null);
        setLoading(false);
      },
      (e) => {
        if (mine !== generation.current) return;
        setError(toAppError(e));
        setLoading(false);
      },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  const setData: Dispatch<SetStateAction<T | null>> = useCallback((value) => {
    generation.current += 1; // whatever read is still in flight is now older than this value
    setLoading(false);
    setRawData(value);
  }, []);
  useEffect(() => {
    alive.current = true;
    load();
    return () => {
      alive.current = false;
      generation.current += 1;
    };
  }, [load]);
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
