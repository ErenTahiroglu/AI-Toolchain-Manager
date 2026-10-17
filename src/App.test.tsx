import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { ErrorBoundary } from "./components/common";
import { createBackend, report, type Backend } from "./test/backend";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};

beforeEach(() => {
  backend = createBackend();
});

describe("navigation", () => {
  it("shows the six approved sections and marks the current one", async () => {
    render(<App />);
    const nav = screen.getByRole("navigation", { name: "Ana gezinme" });
    for (const name of ["Ana Sayfa", "Projeler", "Araç Kütüphanesi", "İstem Kütüphanesi", "Etkinlik ve Denetim", "Ayarlar"]) {
      expect(within(nav).getByRole("button", { name })).toBeInTheDocument();
    }
    expect(within(nav).getByRole("button", { name: "Ana Sayfa" })).toHaveAttribute("aria-current", "page");
    await go("Ayarlar");
    expect(within(nav).getByRole("button", { name: "Ayarlar" })).toHaveAttribute("aria-current", "page");
  });
});

describe("dashboard honesty", () => {
  it("does not claim health before any check ran", async () => {
    render(<App />);
    expect(await screen.findByText(/Henüz sağlık kontrolü çalıştırılmadı/)).toBeInTheDocument();
    expect(screen.queryByText("Tüm kontroller başarılı")).not.toBeInTheDocument();
    expect(screen.getByText(/hiçbir aracın gerçekten çalıştığı doğrulanmış değildir/)).toBeInTheDocument();
  });

  it("an 'unknown' overall result is never shown as success", async () => {
    backend.lastReport = report("unknown");
    render(<App />);
    expect(await screen.findByText(/bazı konular doğrulanamadı/)).toBeInTheDocument();
    expect(screen.queryByText("Tüm kontroller başarılı")).not.toBeInTheDocument();
  });

  it("shows a backend failure as an alert with its code and a retry button", async () => {
    backend.failures.get_dashboard = { code: "DB_UNAVAILABLE", message: "Yerel veritabanı açılamadı." };
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("DB_UNAVAILABLE");
    expect(within(alert).getByRole("button", { name: "Yeniden dene" })).toBeInTheDocument();
  });
});

describe("synthetic and unavailable content", () => {
  it("labels projects as sample data and disables the add button", async () => {
    render(<App />);
    await go("Projeler");
    expect(await screen.findByText("ÖRNEK VERİ")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Proje ekle" })).toBeDisabled();
    expect(screen.getByText("Bilinmiyor (hiçbir şey denetlenmedi)")).toBeInTheDocument();
  });

  it("lists nine tools, none installed, and every Install button is disabled with a reason", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const sections = await screen.findAllByRole("region");
    expect(sections).toHaveLength(9);
    const buttons = screen.getAllByRole("button", { name: "Kur" });
    expect(buttons).toHaveLength(9);
    buttons.forEach((b) => expect(b).toBeDisabled());
    expect(screen.getAllByText("Kurulu değil / yönetilmiyor")).toHaveLength(9);
    expect(screen.queryByText(/Doğrulandı$/)).not.toBeInTheDocument();
  });

  it("lists three read-only planned prompt sources and no usable search/download", async () => {
    render(<App />);
    await go("İstem Kütüphanesi");
    expect(await screen.findAllByText("Salt okunur")).toHaveLength(3);
    expect(screen.getAllByText("Planlı")).toHaveLength(3);
    screen.getAllByRole("button", { name: "İndir / ara" }).forEach((b) => expect(b).toBeDisabled());
  });
});

describe("activity and self-audit", () => {
  it("runs checks and keeps failed / unknown / skipped visibly distinct", async () => {
    backend.nextReport = report("failed");
    render(<App />);
    await go("Etkinlik ve Denetim");
    await userEvent.click(await screen.findByRole("button", { name: "Kontrolleri şimdi çalıştır" }));
    expect(await screen.findByText(/en az bir kontrol başarısız/)).toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Kontrol sonuçları" });
    expect(within(list).getByText("Başarısız")).toBeInTheDocument();
    expect(within(list).getByText("Bilinmiyor")).toBeInTheDocument();
    expect(within(list).getByText("Atlandı")).toBeInTheDocument();
    expect(within(list).getByText("DB_INTEGRITY_FAILED")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.cmd === "run_health_checks")).toBe(true);
  });

  it("reports a failed action instead of swallowing it", async () => {
    backend.failures.run_health_checks = { code: "DB_UNAVAILABLE", message: "kapalı" };
    render(<App />);
    await go("Etkinlik ve Denetim");
    await userEvent.click(await screen.findByRole("button", { name: "Kontrolleri şimdi çalıştır" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("DB_UNAVAILABLE");
  });

  it("verifies the audit chain on demand", async () => {
    render(<App />);
    await go("Etkinlik ve Denetim");
    await userEvent.click(await screen.findByRole("button", { name: "Zinciri doğrula" }));
    expect(await screen.findByText(/1 kayıt tutarlı/)).toBeInTheDocument();
  });
});

describe("settings", () => {
  it("applies and persists the theme through the typed command", async () => {
    render(<App />);
    await go("Ayarlar");
    await userEvent.click(await screen.findByRole("radio", { name: "Koyu" }));
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
    expect(backend.calls).toContainEqual({ cmd: "set_setting", args: { key: "theme", value: "dark" } });
    await userEvent.click(screen.getByRole("radio", { name: "Sistem ayarını kullan" }));
    await waitFor(() => expect(document.documentElement).not.toHaveAttribute("data-theme"));
  });

  it("startup is off by default and only changes through the user's switch", async () => {
    render(<App />);
    await go("Ayarlar");
    const toggle = await screen.findByRole("checkbox", { name: /otomatik başlat/ });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    expect(backend.calls.some((c) => c.cmd === "set_startup_enabled")).toBe(false);
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(backend.calls).toContainEqual({ cmd: "set_startup_enabled", args: { enabled: true } });
  });

  it("shows the real OS state when enabling startup fails", async () => {
    backend.failures.set_startup_enabled = { code: "STARTUP_VERIFY_FAILED", message: "doğrulanamadı" };
    render(<App />);
    await go("Ayarlar");
    const toggle = await screen.findByRole("checkbox", { name: /otomatik başlat/ });
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    expect(await screen.findByRole("alert")).toHaveTextContent("STARTUP_VERIFY_FAILED");
    expect(toggle).not.toBeChecked();
  });

  it("flags a mismatch between the stored preference and the operating system", async () => {
    backend.startup = { preference: true, osEnabled: false };
    render(<App />);
    await go("Ayarlar");
    expect(await screen.findByText(/uyuşmuyor/)).toBeInTheDocument();
  });
});

describe("error boundary", () => {
  it("shows a readable message and writes the crash to the local log", async () => {
    const Bomb = () => {
      throw new Error("kaboom");
    };
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<ErrorBoundary><Bomb /></ErrorBoundary>);
    expect(await screen.findByRole("alert")).toHaveTextContent("beklenmeyen bir hata");
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "report_ui_error" && String(c.args?.message).includes("kaboom"))).toBe(true),
    );
  });
});
