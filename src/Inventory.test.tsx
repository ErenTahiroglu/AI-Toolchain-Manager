import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { createBackend, obs, type Backend } from "./test/backend";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const open = async () => {
  render(<App />);
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name: "Bilgisayar Envanteri" }));
  await screen.findByRole("heading", { level: 1, name: "Bilgisayar Envanteri" });
};
const scanned = (observations = [obs()]) => {
  backend.inventory = {
    lastScan: {
      scanId: "s1", startedAt: "x", finishedAt: new Date().toISOString(), platformTarget: "windows-x64", authorizedPath: true, authorizedUserConfig: false,
      authorizedProject: false, status: "completed", observationCount: observations.length,
    },
    observations,
    requests: [],
    staleAfterHours: 24,
    notChecked: [
      "Claude Code kullanıcı ayarları, eklenti kayıtları ve MCP tanımları okunmadı (yetki verilmedi).",
      "Hiçbir proje klasörüne bakılmadı: projeye özel ayarlar ve .mcp.json için bir klasör seçmeniz gerekir.",
    ],
  };
};

beforeEach(() => {
  backend = createBackend();
});

describe("computer inventory (read-only discovery)", () => {
  it("never scans by itself and says that nothing is known before the first scan", async () => {
    await open();
    expect(await screen.findByText(/Henüz hiç tarama yapılmadı/, { selector: "p" })).toBeInTheDocument();
    expect(calls("run_discovery_scan")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Bu bilgisayarı tara" })).toBeDisabled();
    expect(screen.getByText(/Tarama kendiliğinden hiç başlamaz/)).toBeInTheDocument();
    expect(screen.getByText(/Bulundu ≠ yapılandırıldı ≠ çalışıyor/)).toBeInTheDocument();
  });

  it("explains exactly what each permission reads and does not pre-select any of them", async () => {
    await open();
    const boxes = await screen.findAllByRole("checkbox");
    expect(boxes).toHaveLength(3);
    boxes.forEach((b) => expect(b).not.toBeChecked());
    expect(screen.getByText(/yalnızca “bu adda bir dosya var mı” diye bakılır/)).toBeInTheDocument();
    expect(screen.getByText(/Şifreler, anahtarlar ve diğer içerik saklanmaz/)).toBeInTheDocument();
    expect(screen.getByText(/Yalnızca şu üç dosya okunur/)).toBeInTheDocument();
  });

  it("sends exactly the permissions the user ticked", async () => {
    await open();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Bilinen program adlarını ara/ }));
    await userEvent.click(screen.getByRole("button", { name: "Bu bilgisayarı tara" }));
    expect(calls("run_discovery_scan")[0].args).toEqual({ request: { pathExecutables: true, userConfig: false, project: null } });
    expect(await screen.findByText(/Hiçbir şey kurulmadı, çalıştırılmadı veya değiştirilmedi/)).toBeInTheDocument();
  });

  it("a project scan needs a registered project and a folder, and sends both for this scan only", async () => {
    backend.projects.push({ projectId: "p2", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "x", preferences: [] });
    await open();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Bir proje klasörüne bak/ }));
    const scan = screen.getByRole("button", { name: "Bu bilgisayarı tara" });
    expect(scan).toBeDisabled();
    await screen.findByRole("option", { name: "Benim sitem" });
    expect(screen.queryByRole("option", { name: /Örnek/ })).not.toBeInTheDocument(); // sample projects have no folder
    await userEvent.selectOptions(screen.getByLabelText("Proje kaydı"), "benim-sitem");
    expect(scan).toBeDisabled();
    await userEvent.type(screen.getByLabelText(/Proje klasörünün tam yolu/), "C:\\Projeler\\sitem");
    expect(scan).toBeEnabled();
    await userEvent.click(scan);
    expect(calls("run_discovery_scan")[0].args).toEqual({
      request: { pathExecutables: false, userConfig: false, project: { alias: "benim-sitem", directory: "C:\\Projeler\\sitem" } },
    });
    expect(screen.getByText(/reddedilir/)).toBeInTheDocument();
  });

  it("shows present, referenced-but-missing and unknown states side by side without calling any of them verified", async () => {
    scanned([
      obs({ id: 1, subject: "gitleaks", presence: "present", configuration: "not_applicable" }),
      obs({
        id: 2, subject: "anthropic-frontend-design", agentSurface: "claude-code-terminal", method: "claude-plugin-declaration", presence: "absent",
        configuration: "referenced_not_present", identityVerified: true, confidence: "medium",
        evidence: "Eklenti kullanıcı ayarları içinde bildirilmiş, ancak kurulum kaydında bulunamadı.",
        limitations: ["Çalışan bir Claude Code oturumunda yüklü olup olmadığı bilinmiyor; işlev test edilmedi."],
      }),
      obs({ id: 3, subject: "agent:antigravity-ide", agentSurface: "antigravity-ide", method: "not-documented", presence: "unknown", configuration: "unknown", confidence: "none", evidence: "Belgelenmiş bir algılama yöntemi yok." }),
      obs({
        id: 4, subject: "superpowers", agentSurface: "claude-code-terminal", method: "claude-plugin-declaration", presence: "present", configuration: "configured",
        version: "7.0.0", versionSource: "Claude Code kurulum kaydı", identityVerified: false, confidence: "medium",
      }),
    ]);
    await open();
    const gitleaks = (await screen.findByRole("heading", { name: "gitleaks" })).closest("section")!;
    expect(within(gitleaks).getByText("Bulundu")).toBeInTheDocument();
    expect(within(gitleaks).getByText(/Hayır — işlev test edilmedi/)).toBeInTheDocument();
    const fd = screen.getByRole("heading", { name: "anthropic-frontend-design" }).closest("section")!;
    expect(within(fd).getByText("Bulunamadı")).toBeInTheDocument();
    expect(within(fd).getByText("Başvuru var, dosyalar yok")).toBeInTheDocument();
    const ag = screen.getByRole("heading", { name: /Antigravity masaüstü IDE/ }).closest("section")!;
    expect(within(ag).getByText("Bilinmiyor", { selector: ".badge" })).toBeInTheDocument();
    const sp = screen.getByRole("heading", { name: "superpowers" }).closest("section")!;
    expect(within(sp).getByText(/7\.0\.0/)).toBeInTheDocument();
    expect(within(sp).getByText(/Kimlik doğrulandı: hayır/)).toBeInTheDocument();
    expect(screen.queryByText(/işlev doğrulandı: evet/i)).not.toBeInTheDocument();
    expect(document.querySelector(".badge-passed")).toBeNull(); // no green badge anywhere in the inventory
  });

  it("has no install, update or run button anywhere on the page", async () => {
    scanned();
    await open();
    await screen.findByRole("heading", { name: "gitleaks" });
    const labels = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    for (const forbidden of [/\bkur\b/i, /yükle/i, /install/i, /güncelle/i, /çalıştır/i, /etkinleştir/i]) {
      expect(labels.filter((l) => forbidden.test(l)), String(forbidden)).toEqual([]);
    }
  });

  it("marks old observations as stale and offers a refresh", async () => {
    scanned([obs({ stale: true, ageHours: 49, observedAt: new Date(Date.now() - 49 * 3600_000).toISOString() })]);
    await open();
    expect(await screen.findByText(/Bazı gözlemler 24 saatten eski/)).toBeInTheDocument();
    expect(screen.getByText(/Eski \(49 saat\)/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Yeniden tara" })).toBeInTheDocument();
  });

  it("lists what was not checked, including the missing project permission", async () => {
    scanned();
    await open();
    const section = (await screen.findByRole("heading", { name: "Bakılmayanlar" })).closest("section")!;
    expect(within(section).getByText(/Hiçbir proje klasörüne bakılmadı/)).toBeInTheDocument();
    expect(within(section).getByText(/okunmadı \(yetki verilmedi\)/)).toBeInTheDocument();
  });

  it("keeps the user's request and the observation apart", async () => {
    scanned();
    backend.inventory.requests = [
      { entryId: "anthropic-frontend-design", entryName: "Anthropic Frontend Design", acceptedProjects: 2, observed: "absent", note: "Etkinleştirmek istediniz, ancak bu bilgisayarda bulunamadı." },
    ];
    await open();
    const item = (await screen.findByText(/2 proje için etkinleştirmek istediniz/)).closest("li")!;
    expect(item).toHaveTextContent("Bulunamadı");
    expect(screen.getByText(/aracın kurulu olduğu anlamına gelmez/)).toBeInTheDocument();
  });

  it("shows a refused scan as an error with its code and records nothing", async () => {
    backend.failures.run_discovery_scan = { code: "PROJECT_DIR_NETWORK", message: "Ağ paylaşımları ve aygıt yolları taranmaz." };
    backend.projects.push({ projectId: "p2", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "x", preferences: [] });
    await open();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Bir proje klasörüne bak/ }));
    await screen.findByRole("option", { name: "Benim sitem" });
    await userEvent.selectOptions(screen.getByLabelText("Proje kaydı"), "benim-sitem");
    await userEvent.type(screen.getByLabelText(/Proje klasörünün tam yolu/), "\\\\sunucu\\paylasim\\proje");
    await userEvent.click(screen.getByRole("button", { name: "Bu bilgisayarı tara" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("PROJECT_DIR_NETWORK");
    await waitFor(() => expect(screen.queryByText(/Tarama tamamlandı/)).not.toBeInTheDocument());
  });
});
