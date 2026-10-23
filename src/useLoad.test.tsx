// PRODUCT-02 / P01-RT-01 - useLoad is "latest request wins": no stale success, stale error or stale `loading` bookkeeping can reach the screen.
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useLoad } from "./components/common";

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (v: T) => void;
  reject: (e: unknown) => void;
}
const deferred = <T,>(): Deferred<T> => {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
};
/** A loader whose n-th call returns the n-th deferred promise. */
const sequence = <T,>() => {
  const calls: Deferred<T>[] = [];
  const loader = () => {
    const d = deferred<T>();
    calls.push(d);
    return d.promise;
  };
  return { calls, loader };
};
const settle = () => act(async () => { await Promise.resolve(); });

afterEach(() => vi.restoreAllMocks());

describe("useLoad: the existing contract", () => {
  it("starts loading, then exposes data, no error, and the same API (data, error, loading, reload, setData)", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    expect(result.current.loading).toBe(true);
    expect(result.current.data).toBeNull();
    expect(Object.keys(result.current).sort()).toEqual(["data", "error", "loading", "reload", "setData"]);
    await act(async () => s.calls[0].resolve("A"));
    expect(result.current).toMatchObject({ data: "A", error: null, loading: false });
  });

  it("turns a rejection into an AppError and a synchronous throw into one as well", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    await act(async () => s.calls[0].reject({ code: "X_FAILED", message: "kötü" }));
    expect(result.current.error).toEqual({ code: "X_FAILED", message: "kötü" });
    expect(result.current.loading).toBe(false);
    const { result: r2 } = renderHook(() => useLoad<string>(() => { throw new Error("sync"); }));
    await waitFor(() => expect(r2.current.error).toEqual({ code: "IPC_FAILED", message: "Uygulama çekirdeğiyle iletişim kurulamadı." }));
    expect(r2.current.loading).toBe(false);
  });

  it("setData still replaces the data directly", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    await act(async () => s.calls[0].resolve("A"));
    act(() => result.current.setData("manual"));
    expect(result.current.data).toBe("manual");
  });

  it("a successful reload clears an earlier error", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    await act(async () => s.calls[0].reject({ code: "E", message: "m" }));
    expect(result.current.error).not.toBeNull();
    act(() => result.current.reload());
    await act(async () => s.calls[1].resolve("B"));
    expect(result.current).toMatchObject({ data: "B", error: null, loading: false });
  });
});

describe("useLoad: out-of-order and overlapping loads", () => {
  it("resolve A then B: the result is B", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    act(() => result.current.reload());
    await act(async () => s.calls[0].resolve("A"));
    expect(result.current.data).toBeNull(); // A is already stale: B is the current request
    expect(result.current.loading).toBe(true);
    await act(async () => s.calls[1].resolve("B"));
    expect(result.current).toMatchObject({ data: "B", loading: false });
  });

  it("resolve B then A: the late A never overwrites B", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    act(() => result.current.reload());
    await act(async () => s.calls[1].resolve("B"));
    expect(result.current).toMatchObject({ data: "B", loading: false });
    await act(async () => s.calls[0].resolve("A"));
    expect(result.current).toMatchObject({ data: "B", error: null, loading: false });
  });

  it("a stale rejection after a newer success changes nothing", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    act(() => result.current.reload());
    await act(async () => s.calls[1].resolve("B"));
    await act(async () => s.calls[0].reject({ code: "STALE", message: "eski hata" }));
    expect(result.current).toMatchObject({ data: "B", error: null, loading: false });
  });

  it("a stale success after a newer failure does not hide the failure or install old data", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    act(() => result.current.reload());
    await act(async () => s.calls[1].reject({ code: "NEW_FAILED", message: "yeni hata" }));
    expect(result.current.error).toEqual({ code: "NEW_FAILED", message: "yeni hata" });
    await act(async () => s.calls[0].resolve("A"));
    expect(result.current.data).toBeNull();
    expect(result.current.error).toEqual({ code: "NEW_FAILED", message: "yeni hata" });
    expect(result.current.loading).toBe(false);
  });

  it("an earlier load that finishes while a newer one is pending does not end the loading state", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    act(() => result.current.reload());
    act(() => result.current.reload());
    expect(s.calls).toHaveLength(3);
    await act(async () => s.calls[0].resolve("A"));
    await act(async () => s.calls[1].reject({ code: "E", message: "m" }));
    expect(result.current.loading).toBe(true); // only the third call may end it
    expect(result.current.data).toBeNull();
    expect(result.current.error).toBeNull();
    await act(async () => s.calls[2].resolve("C"));
    expect(result.current).toMatchObject({ data: "C", error: null, loading: false });
  });

  it("many immediate reloads resolved in a scrambled order always end on the last request", async () => {
    const s = sequence<number>();
    const { result } = renderHook(() => useLoad(s.loader));
    for (let i = 0; i < 5; i++) act(() => result.current.reload());
    expect(s.calls).toHaveLength(6);
    for (const i of [3, 0, 5, 1, 4, 2]) await act(async () => s.calls[i].resolve(i));
    expect(result.current).toMatchObject({ data: 5, loading: false, error: null });
  });

  it("a stale error does not clear a newer successful result's data on the next stale success either", async () => {
    const s = sequence<string>();
    const { result } = renderHook(() => useLoad(s.loader));
    await act(async () => s.calls[0].resolve("A"));
    act(() => result.current.reload());
    act(() => result.current.reload());
    await act(async () => s.calls[2].resolve("C"));
    await act(async () => s.calls[1].resolve("B"));
    expect(result.current.data).toBe("C");
  });
});

describe("useLoad: unmount and changing inputs", () => {
  it("a promise that settles after unmount changes nothing and raises no React warning", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const s = sequence<string>();
    const { result, unmount } = renderHook(() => useLoad(s.loader));
    const before = result.current;
    unmount();
    await act(async () => s.calls[0].resolve("A"));
    await act(async () => { await Promise.resolve(); });
    expect(result.current).toBe(before);
    expect(result.current.data).toBeNull();
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it("a rejection after unmount is swallowed the same way", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const s = sequence<string>();
    const { unmount } = renderHook(() => useLoad(s.loader));
    unmount();
    await act(async () => s.calls[0].reject({ code: "E", message: "m" }));
    expect(errorSpy).not.toHaveBeenCalled();
  });

  it("reload() after unmount starts nothing", () => {
    const s = sequence<string>();
    const { result, unmount } = renderHook(() => useLoad(s.loader));
    const reload = result.current.reload;
    unmount();
    reload();
    expect(s.calls).toHaveLength(1);
  });

  it("changing deps starts a new load and the load for the old deps is ignored even if it finishes last", async () => {
    const calls: { key: string; d: Deferred<string> }[] = [];
    const { result, rerender } = renderHook(({ key }) => useLoad(() => { const d = deferred<string>(); calls.push({ key, d }); return d.promise; }, [key]), { initialProps: { key: "one" } });
    rerender({ key: "two" });
    expect(calls.map((c) => c.key)).toEqual(["one", "two"]);
    await act(async () => calls[1].d.resolve("two-data"));
    await act(async () => calls[0].d.resolve("one-data"));
    expect(result.current.data).toBe("two-data");
  });

  it("reload() uses the newest loader, not the closure of the first render", async () => {
    const used: string[] = [];
    const { result, rerender } = renderHook(({ tag }) => useLoad(() => { used.push(tag); return Promise.resolve(tag); }), { initialProps: { tag: "first" } });
    await settle();
    rerender({ tag: "second" }); // same (empty) deps: no automatic reload ...
    expect(used).toEqual(["first"]);
    act(() => result.current.reload()); // ... but a manual one must not use the stale "first" loader
    await waitFor(() => expect(result.current.data).toBe("second"));
    expect(used).toEqual(["first", "second"]);
  });
});
