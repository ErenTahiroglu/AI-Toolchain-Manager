import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { createBackend, obs, type Backend } from "./test/backend";
import type { Ladder } from "./types";

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
    scanned([obs({ stale: true, current: false, notCurrentReason: "Son tarama 24 saatten eski; yeniden tarayın.", provenance: "latest_scan", ageHours: 49, observedAt: new Date(Date.now() - 49 * 3600_000).toISOString() })]);
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

  it("keeps the user's request and the observation apart, one row per project, surface and platform", async () => {
    scanned();
    backend.inventory.requests = [
      {
        entryId: "anthropic-frontend-design", entryName: "Anthropic Frontend Design", projectAlias: "web", projectName: "Web sitesi",
        agentSurface: "claude-code-terminal", platformTarget: "windows-x64", status: "absent_current",
        note: "Etkinleştirmek istediniz, ancak son taramada bu bilgisayarda bulunamadı.",
        basis: { scanId: "abcdef0123456789", finishedAt: new Date().toISOString(), authorizedPath: true, authorizedUserConfig: true, authorizedProject: false },
      },
      {
        entryId: "gitleaks", entryName: "Gitleaks", projectAlias: "web", projectName: "Web sitesi", agentSurface: "local-cli", platformTarget: "macos-arm64",
        status: "platform_mismatch", note: "Yalnızca başka bir işletim sistemi/mimari için gözlem var.", basis: null,
      },
    ];
    await open();
    const item = (await screen.findByText(/Anthropic Frontend Design/, { selector: "strong" })).closest("li")!;
    expect(item).toHaveTextContent("Web sitesi");
    expect(item).toHaveTextContent("Claude Code (terminal)");
    expect(item).toHaveTextContent("Son taramada bulunamadı");
    expect(item).toHaveTextContent("Tarama abcdef01");
    expect(item).toHaveTextContent("program adları (PATH) + Claude Code kullanıcı dosyaları");
    const other = screen.getByText(/Gitleaks/, { selector: "strong" }).closest("li")!;
    expect(other).toHaveTextContent("Başka bir işletim sistemi için gözlem var");
    expect(screen.getByText(/aracın kurulu olduğu anlamına gelmez/)).toBeInTheDocument();
  });

  it("never presents an earlier result as a current answer for a request", async () => {
    scanned();
    backend.inventory.requests = [
      {
        entryId: "gitleaks", entryName: "Gitleaks", projectAlias: "web", projectName: "Web sitesi", agentSurface: "local-cli", platformTarget: "windows-x64",
        status: "previously_present", note: "Eski sonuç (Son taramada bu bölüm denetlenmedi (yetki verilmedi)). Yeniden tarayın; eski bir sonuç hiçbir işlem için dayanak sayılmaz.",
        basis: { scanId: "11111111-aaaa", finishedAt: "2026-10-16T09:00:00Z", authorizedPath: true, authorizedUserConfig: false, authorizedProject: false },
      },
    ];
    await open();
    const item = (await screen.findByText(/Eski taramada bulunmuştu \(güncel değil\)/)).closest("li")!;
    expect(item).toHaveTextContent("dayanak sayılmaz");
    expect(item).not.toHaveTextContent("Son taramada bulundu");
  });

  it("shows current and earlier observations in separate sections, with the scan and permissions of each", async () => {
    scanned([
      obs({ id: 1, subject: "strix", scanId: "cccccccc-1", scanAuthorizedPath: true, scanAuthorizedUserConfig: true }),
      obs({
        id: 2, subject: "gitleaks", scanId: "bbbbbbbb-2", provenance: "earlier_scan", current: false, latestScanChecked: false,
        notCurrentReason: "Son taramada bu bölüm denetlenmedi (yetki verilmedi); bu, eski bir taramanın sonucudur.",
        scanAuthorizedPath: true, scanAuthorizedUserConfig: false, scanAuthorizedProject: false,
      }),
    ]);
    await open();
    const current = (await screen.findByRole("heading", { name: /Güncel gözlemler/ })).closest("section")!;
    const earlier = screen.getByRole("heading", { name: /Önceki taramalardan — güncel değil/ }).closest("section")!;
    expect(within(current).getByRole("heading", { name: "strix" })).toBeInTheDocument();
    expect(within(current).queryByRole("heading", { name: "gitleaks" })).not.toBeInTheDocument();
    expect(within(earlier).getByRole("heading", { name: "gitleaks" })).toBeInTheDocument();
    expect(within(earlier).queryByRole("heading", { name: "strix" })).not.toBeInTheDocument();
    expect(within(earlier).getByText(/yetki verilmedi\); bu, eski bir taramanın sonucudur/)).toBeInTheDocument();
    expect(within(earlier).getByText(/Tarama bbbbbbbb/)).toHaveTextContent("izinler: program adları (PATH)");
    expect(within(earlier).getByText(/hiçbir işlem için dayanak sayılmaz/)).toBeInTheDocument();
  });

  it("labels records that were scrubbed by the privacy migration and shows only grammar-shaped locations", async () => {
    scanned([obs({ legacy: true, location: "eski kayıt: konum gizlilik nedeniyle kaldırıldı" })]);
    await open();
    expect(await screen.findByText(/Eski kayıt biçimi — gizlilik için konum\/ad bilgisi temizlendi/)).toBeInTheDocument();
    expect(screen.getByText(/Yer: eski kayıt: konum gizlilik nedeniyle kaldırıldı/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/[A-Za-z]:\\Users|\/Users\//);
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

  const ladder = (over: Partial<Ladder>): Ladder => ({
    assessed: true, filenameDetected: true, installRecord: false, publisher: "none", identity: "unknown", version: "unknown", versionTrust: "none",
    signatureBlock: "not_assessed", notPerformed: ["active_version_probe"], ...over,
  });

  it("shows each rung of the evidence ladder separately and never a green success for a merely detected tool", async () => {
    scanned([
      obs({ id: 1, subject: "gitleaks" }),
      obs({
        id: 2, subject: "anthropic-frontend-design", agentSurface: "claude-code-terminal", method: "claude-plugin-declaration", configuration: "configured",
        identityVerified: true, version: "1.0.0", versionSource: "Claude Code kurulum kaydı",
        ladder: ladder({ installRecord: true, publisher: "marketplace", identity: "validated", version: "known", versionTrust: "trusted", notPerformed: [] }),
      }),
      obs({
        id: 3, subject: "agent-browser", ladder: ladder({ installRecord: true, publisher: "self_declared", identity: "evidence_available", version: "known", versionTrust: "declared" }),
        version: "0.39.1", versionSource: "npm paket kaydı",
      }),
    ]);
    await open();
    const g = (await screen.findByRole("heading", { name: "gitleaks" })).closest("section")!;
    const gl = within(g).getByRole("list", { name: "Kanıt basamakları" });
    for (const t of ["Tespit edildi", "Yapılandırma kanıtı yok", "Kimlik kanıtı yok", "Kimlik doğrulanmadı", "Sürüm bilinmiyor", "İşlev test edilmedi"]) {
      expect(within(gl).getByText(new RegExp(t))).toBeInTheDocument();
    }
    const fd = screen.getByRole("heading", { name: "anthropic-frontend-design" }).closest("section")!;
    const fl = within(fd).getByRole("list", { name: "Kanıt basamakları" });
    expect(within(fl).getByText(/Kimlik doğrulandı \(kayıt \+ sabitlenmiş kaynak\)/)).toBeInTheDocument();
    expect(within(fl).getByText(/Sürüm biliniyor \(kurulum kaydından\)/)).toBeInTheDocument();
    expect(within(fl).getByText(/İşlev test edilmedi/)).toBeInTheDocument();
    const ab = screen.getByRole("heading", { name: "agent-browser" }).closest("section")!;
    const al = within(ab).getByRole("list", { name: "Kanıt basamakları" });
    expect(within(al).getByText(/Kimlik kanıtı mevcut/)).toBeInTheDocument();
    expect(within(al).getByText(/Kimlik doğrulanmadı/)).toBeInTheDocument();
    expect(within(al).getByText(/paketin kendi beyanından/)).toBeInTheDocument();
    expect(document.querySelector(".badge-passed")).toBeNull();
    expect(screen.queryByText(/işlev doğrulandı: evet/i)).not.toBeInTheDocument();
  });

  it("links every conclusion to its local evidence text and lists the checks that were not performed, with reasons", async () => {
    scanned([obs({ id: 7, ladder: ladder({ notPerformed: ["active_version_probe", "signature_chain_validation", "installed_file_digest"], signatureBlock: "present_unvalidated" }) })]);
    await open();
    const section = (await screen.findByRole("heading", { name: "gitleaks" })).closest("section")!;
    const links = within(section).getAllByRole("link", { name: "(kanıt)" });
    expect(links.length).toBeGreaterThan(3);
    links.forEach((a) => expect(a).toHaveAttribute("href", "#ev-7"));
    expect(section.querySelector("#ev-7")).not.toBeNull();
    const summary = within(section).getByText(/Yapılmayan kontroller \(3\)/);
    await userEvent.click(summary);
    expect(within(section).getByText(/hiçbir program çalıştırılmadı/)).toBeInTheDocument();
    expect(within(section).getByText(/imza bloğunun varlığı güven anlamına gelmez/)).toBeInTheDocument();
    expect(within(section).getByText(/katalogda kurulu dosya için sabitlenmiş özet yok/)).toBeInTheDocument();
  });

  it("reports conflicting observations and stale ones as their own states, and old records as not assessed", async () => {
    scanned([
      obs({ id: 1, subject: "agent-browser", ladder: ladder({ identity: "conflicted", installRecord: true }) }),
      obs({ id: 2, subject: "strix", stale: true, current: false, notCurrentReason: "Son tarama 24 saatten eski; yeniden tarayın.", ladder: ladder({}) }),
      obs({ id: 3, subject: "markitdown", legacy: true, ladder: ladder({ assessed: false }) }),
    ]);
    await open();
    const ab = (await screen.findByRole("heading", { name: "agent-browser" })).closest("section")!;
    expect(within(ab).getByText(/Çelişkili gözlemler var/)).toBeInTheDocument();
    const strix = screen.getByRole("heading", { name: "strix" }).closest("section")!;
    expect(within(strix).getByText(/Eski \/ güncel olmayan gözlem/)).toBeInTheDocument();
    const md = screen.getByRole("heading", { name: "markitdown" }).closest("section")!;
    expect(within(md).getByText(/Kanıt basamakları bu kayıt için değerlendirilmedi/)).toBeInTheDocument();
    expect(within(md).queryByRole("list", { name: "Kanıt basamakları" })).not.toBeInTheDocument();
  });

  it("previews old backups without any way to delete them and states the retention trade-off", async () => {
    backend.backupPlan = {
      entries: [{ fileName: "atm-v4-20261016T100000Z.sqlite", schemaVersion: 4, sizeBytes: 204800, modified: "2026-10-16T10:00:00Z", mayHoldOldPersonalData: "yes", rowsFailingPrivacyGate: 4 }],
      preconditions: [{ id: "integrity", ok: true, text: "SQLite bütünlük denetimi (integrity_check) başarılı." }],
      candidates: ["atm-v4-20261016T100000Z.sqlite"], keep: [], deletionAvailable: false, executed: false,
      notes: ["Bu bir ÖNİZLEMEDİR: hiçbir yedek silinmedi, taşınmadı veya değiştirilmedi.", "Dosyanın silinmesi, içeriğin fiziksel olarak yok edildiğini garanti etmez: SSD'lerde aşınma dengeleme olabilir."],
    };
    await open();
    expect(await screen.findByText(/birebir kopyasıdır/)).toBeInTheDocument();
    expect(calls("preview_backup_cleanup")).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Yedekleri incele" }));
    expect(await screen.findByText("atm-v4-20261016T100000Z.sqlite")).toBeInTheDocument();
    expect(screen.getByText(/Evet \(4 kayıt\)/)).toBeInTheDocument();
    expect(screen.getByText(/hiçbir yedek silinmedi/)).toBeInTheDocument();
    expect(screen.getByText(/fiziksel olarak yok edildiğini garanti etmez/)).toBeInTheDocument();
    expect(screen.getByText(/Silme bu sürümde yoktur/)).toBeInTheDocument();
    const labels = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    expect(labels.filter((l) => /sil|kaldır|temizle|delete|remove/i.test(l))).toEqual([]);
    expect(document.body.textContent).not.toMatch(/[A-Za-z]:\Users|\/Users\//);
  });
});
