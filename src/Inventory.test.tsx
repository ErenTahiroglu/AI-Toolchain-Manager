import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { createBackend, obs, type Backend } from "./test/backend";
import type { ArtifactEvidence, BackupEntry, Ladder } from "./types";

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
      authorizedProject: false, authorizedFingerprint: false, status: "completed", observationCount: observations.length,
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

  it("makes no absolute network promise: the scan starts no connection on purpose, the WebView runtime is named as outside that statement (#25)", async () => {
    await open();
    const banner = (await screen.findByText(/Bulundu ≠ yapılandırıldı ≠ çalışıyor/)).closest('[role="note"]');
    expect(banner).not.toBeNull();
    const text = (banner!.textContent ?? "").replace(/\s+/g, " ");
    for (const needle of ["bu tarama bilerek ağ bağlantısı başlatmaz", "WebView çalışma ortamının ayrı ağ trafiği", "dışlanmış değildir", "(açık sorun #25)", "olmadığı da kanıtlanmamıştır", "Hiçbir program çalıştırılmaz"]) {
      expect(text, needle).toContain(needle);
    }
    // the old unqualified promise is gone (same guard as the CONFIG-02C gate; Issue #25 is open)
    for (const absolute of ["internete bağlanılmaz", "İnternete bağlanılmaz", "internete istek", "ağa hiçbir", "hiçbir ağ"]) expect(text, absolute).not.toContain(absolute);
  });

  it("explains exactly what each permission reads and does not pre-select any of them", async () => {
    await open();
    const boxes = await screen.findAllByRole("checkbox");
    expect(boxes).toHaveLength(4);
    boxes.forEach((b) => expect(b).not.toBeChecked());
    expect(screen.getByText(/yalnızca “bu adda bir dosya var mı” diye bakılır/)).toBeInTheDocument();
    expect(screen.getByText(/Şifreler, anahtarlar ve diğer içerik saklanmaz/)).toBeInTheDocument();
    expect(screen.getByText(/Yalnızca şu üç dosya okunur/)).toBeInTheDocument();
  });

  it("sends exactly the permissions the user ticked", async () => {
    await open();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Bilinen program adlarını ara/ }));
    await userEvent.click(screen.getByRole("button", { name: "Bu bilgisayarı tara" }));
    expect(calls("run_discovery_scan")[0].args).toEqual({ request: { pathExecutables: true, userConfig: false, project: null, fingerprintArtifacts: false } });
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
      request: { pathExecutables: false, userConfig: false, project: { alias: "benim-sitem", directory: "C:\\Projeler\\sitem" }, fingerprintArtifacts: false },
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
        basis: { scanId: "abcdef0123456789", finishedAt: new Date().toISOString(), authorizedPath: true, authorizedUserConfig: true, authorizedProject: false, authorizedFingerprint: false },
        origin: { projectRelation: "computer_wide", surfaceRelation: "exact_surface", observedAt: "2026-10-23T09:00:00Z", ageHours: 3 },
      },
      {
        entryId: "gitleaks", entryName: "Gitleaks", projectAlias: "web", projectName: "Web sitesi", agentSurface: "local-cli", platformTarget: "macos-arm64",
        status: "platform_mismatch", note: "Yalnızca başka bir işletim sistemi/mimari için gözlem var.", basis: null, origin: null,
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
        basis: { scanId: "11111111-aaaa", finishedAt: "2026-10-16T09:00:00Z", authorizedPath: true, authorizedUserConfig: false, authorizedProject: false, authorizedFingerprint: false },
        origin: { projectRelation: "computer_wide", surfaceRelation: "exact_surface", observedAt: "2026-10-16T09:00:00Z", ageHours: 200 },
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

  const art = (over: Partial<ArtifactEvidence> = {}): ArtifactEvidence => ({
    policyVersion: 3, fingerprint: "not_requested", unknownReason: null, sha256: null, bytes: null, verifiedFormat: null, recordLink: "none", reference: "none", referenceKind: null,
    matchResult: "not_applicable", referenceEpoch: null, invalidated: false, ...over,
  });
  const ladder = (over: Partial<Ladder>): Ladder => ({
    assessed: true, filenameDetected: true, installRecord: false, publisher: "none", identity: "unknown", version: "unknown", versionTrust: "none",
    signatureBlock: "not_assessed", installedFilesSeen: false, artifactAuth: "none", reclassified: false, artifact: art(), notPerformed: ["active_version_probe"], ...over,
  });

  it("shows each rung of the evidence ladder separately and never a green success for a merely detected tool", async () => {
    scanned([
      obs({ id: 1, subject: "gitleaks" }),
      obs({
        id: 2, subject: "anthropic-frontend-design", agentSurface: "claude-code-terminal", method: "claude-plugin-declaration", configuration: "configured",
        identityVerified: false, version: "1.0.0", versionSource: "Claude Code kurulum kaydı",
        ladder: ladder({ installRecord: true, installedFilesSeen: true, publisher: "marketplace", identity: "record_corroborated", version: "known", versionTrust: "installer_recorded", notPerformed: ["artifact_authentication"] }),
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
    // A partially corroborated record must never read as verified.
    expect(within(fl).getByText(/Kurulum kaydı katalogla uyuşuyor ve dosyalar görünüyor \(yerel, düzenlenebilir kayıt; kurulu içerik doğrulanmadı\)/)).toBeInTheDocument();
    expect(within(fl).getByText(/^Kimlik doğrulanmadı/)).toBeInTheDocument();
    expect(within(fl).queryByText(/^Kimlik doğrulandı/)).not.toBeInTheDocument();
    expect(within(fl).getByText(/Sürüm biliniyor \(yerel kurulum kaydının beyanı; doğrulanmadı\)/)).toBeInTheDocument();
    expect(within(fl).getByText(/İşlev test edilmedi/)).toBeInTheDocument();
    const ab = screen.getByRole("heading", { name: "agent-browser" }).closest("section")!;
    const al = within(ab).getByRole("list", { name: "Kanıt basamakları" });
    expect(within(al).getByText(/Kimlik kanıtı mevcut/)).toBeInTheDocument();
    expect(within(al).getByText(/Kimlik doğrulanmadı/)).toBeInTheDocument();
    expect(within(al).getByText(/paketin kendi beyanı; doğrulanmadı/)).toBeInTheDocument();
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

  it("previews old backups with honest coverage: nothing is called clean, safe or deletable", async () => {
    const entry = (over: Partial<BackupEntry>): BackupEntry => ({
      fileName: "atm-v4-20261016T100000Z.sqlite", schemaVersion: 4, sizeBytes: 204800, modified: "2026-10-16T10:00:00Z", privacyStatus: "violations_observed" as const,
      reason: "findings_in_examined_content", coverage: "complete", layers: [{ layer: "raw_main", outcome: "completed", reason: "all_bytes_inspected" }], observationRowsFailingGate: 4, otherTextFindings: 2, tablesWithFindings: ["diagnostics"], rawPathLikeMatches: 6, rawSensitiveShapeLayers: 0, hasWalSidecar: false, ...over,
    });
    backend.backupPlan = {
      entries: [
        entry({}),
        entry({ fileName: "atm-v2-20261001T000000Z.sqlite", schemaVersion: 2, privacyStatus: "potentially_sensitive", observationRowsFailingGate: null, otherTextFindings: 0, tablesWithFindings: [], rawPathLikeMatches: 0 }),
        entry({ fileName: "atm-v5-20261007T000000Z.sqlite", schemaVersion: 5, privacyStatus: "none_detected_in_examined_content", observationRowsFailingGate: 0, otherTextFindings: 0, tablesWithFindings: [], rawPathLikeMatches: 0 }),
        entry({ fileName: "atm-v5-20261005T000000Z.sqlite", schemaVersion: 5, privacyStatus: "not_assessable", coverage: "incomplete", layers: [{ layer: "raw_wal", outcome: "unreadable", reason: "redirected" }], observationRowsFailingGate: null, otherTextFindings: 0, tablesWithFindings: [], rawPathLikeMatches: 0 }),
        entry({
          fileName: "atm-v5-20261008T000000Z.sqlite", schemaVersion: 5, privacyStatus: "partially_scanned_no_findings", coverage: "partial",
          layers: [{ layer: "all_text", outcome: "partial", reason: "cell_limit" }, { layer: "wal_logical_sql", outcome: "partial", reason: "wal_not_replayed" }], observationRowsFailingGate: 0, otherTextFindings: 0, tablesWithFindings: [], rawPathLikeMatches: 0,
        }),
      ],
      preconditions: [{ id: "integrity", ok: true, text: "SQLite bütünlük denetimi (integrity_check) başarılı." }],
      examined: ["Tüm tablolardaki tüm metin değerleri: özel klasör yolu, gizli anahtar/parola/e-posta biçimleri (örüntü taraması)"],
      notExamined: ["Yedek dışındaki kopyalar: dosya sistemi anlık görüntüleri, gölge kopyalar, çöp kutusu, bulut eşitleme"],
      reviewCandidates: ["atm-v4-20261016T100000Z.sqlite", "atm-v2-20261001T000000Z.sqlite"], noViolationDetected: ["atm-v5-20261007T000000Z.sqlite"],
      partiallyScanned: ["atm-v5-20261008T000000Z.sqlite"], notAssessable: ["atm-v5-20261005T000000Z.sqlite"], directoryStatus: "ok", limits: { maxTextCells: 500000, maxRawBytes: 268435456, maxSqliteImageBytes: 134217728, timeoutSeconds: 30 }, unrecognisedFiles: 1, deletionAvailable: false, executed: false,
      notes: ["Bu bir ÖNİZLEMEDİR: hiçbir yedek silinmedi, taşınmadı veya değiştirilmedi.", "Dosyanın silinmesi, içeriğin fiziksel olarak yok edildiğini garanti etmez: SSD'lerde aşınma dengeleme olabilir."],
    };
    await open();
    expect(await screen.findByText(/birebir kopyasıdır/)).toBeInTheDocument();
    expect(calls("preview_backup_cleanup")).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Yedekleri incele" }));
    expect(await screen.findByText("atm-v4-20261016T100000Z.sqlite")).toBeInTheDocument();
    // coverage is stated up front, and "nothing found" is never "clean"
    expect(screen.getByText(/Bu tarama bir örüntü taramasıdır/)).toBeInTheDocument();
    expect(screen.getByText(/kişisel veri içermediği veya silinmesinin güvenli olduğu anlamına gelmez/)).toBeInTheDocument();
    await userEvent.click(screen.getByText(/Neler incelendi, neler incelenmedi/));
    expect(screen.getByText(/örüntü taraması/, { selector: "li" })).toBeInTheDocument();
    expect(screen.getByText(/dosya sistemi anlık görüntüleri/, { selector: "li" })).toBeInTheDocument();
    const rows = Object.fromEntries(screen.getAllByRole("row").slice(1).map((r) => [r.querySelector("th")!.textContent, r.textContent ?? ""]));
    expect(rows["atm-v4-20261016T100000Z.sqlite"]).toMatch(/İhlal gözlendi/);
    expect(rows["atm-v4-20261016T100000Z.sqlite"]).toMatch(/tablo: diagnostics/);
    expect(rows["atm-v2-20261001T000000Z.sqlite"]).toMatch(/Eski kişisel bilgi taşıyabilir/);
    expect(rows["atm-v2-20261001T000000Z.sqlite"]).toMatch(/Gözlem tablosu: incelenemedi/);
    expect(rows["atm-v5-20261007T000000Z.sqlite"]).toMatch(/İncelenen içerikte ihlal bulunmadı \(kişisel veri içermediği anlamına GELMEZ\)/);
    expect(rows["atm-v5-20261005T000000Z.sqlite"]).toMatch(/BİLİNMİYOR/);
    expect(rows["atm-v5-20261005T000000Z.sqlite"]).toMatch(/-wal dosyası \(ham baytlar\): OKUNAMADI \(bağlantı\/yönlendirme reddedildi\)/);
    // a partial scan never reads as complete
    expect(rows["atm-v5-20261008T000000Z.sqlite"]).toMatch(/KISMEN tarandı, taranan bölümde bir şey bulunmadı \(tamamı incelenmedi\)/);
    expect(rows["atm-v5-20261008T000000Z.sqlite"]).toMatch(/KISMİ tarama/);
    expect(rows["atm-v5-20261008T000000Z.sqlite"]).toMatch(/tüm metin değerleri: KISMİ \(hücre sınırı\)/);
    expect(rows["atm-v5-20261008T000000Z.sqlite"]).not.toMatch(/Tüm katmanlar tamamlandı/);
    // transactions that live only in the -wal file are not claimed as inspected rows
    expect(rows["atm-v5-20261008T000000Z.sqlite"]).toMatch(/-wal içindeki işlemler \(SQL satırı olarak\): KISMİ \(işlenmemiş işlemler olabilir; SQL olarak İNCELENMEDİ\)/);
    expect(screen.getByText(/en çok 500\.000 metin hücresi/)).toBeInTheDocument();
    expect(screen.getByText(/Sınır aşılırsa sonuç “kısmi” olur/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/güvenle silinebilir|temiz yedek|kişisel veri içermiyor|\bsafe\b/i);
    expect(screen.getByText(/hiçbir yedek silinmedi/)).toBeInTheDocument();
    expect(screen.getByText(/fiziksel olarak yok edildiğini garanti etmez/)).toBeInTheDocument();
    expect(screen.getByText(/Silme bu sürümde yoktur/)).toBeInTheDocument();
    const labels = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    expect(labels.filter((l) => /sil|kaldır|temizle|delete|remove/i.test(l))).toEqual([]);
    expect(document.body.textContent).not.toMatch(/[A-Za-z]:\\Users|\/Users\//);
  });

  it("never shows a reclassified or corroborated record as verified, and says the trust was lowered", async () => {
    scanned([
      obs({ id: 1, subject: "caveman", identityVerified: false, ladder: ladder({ installRecord: true, identity: "evidence_available", reclassified: true, version: "known", versionTrust: "installer_recorded" }), version: "3.2.0", versionSource: "Claude Code kurulum kaydı" }),
      obs({ id: 2, subject: "ponytail", identityVerified: false, ladder: ladder({ installRecord: true, installedFilesSeen: false, publisher: "marketplace", identity: "evidence_available", notPerformed: ["artifact_authentication"] }) }),
    ]);
    await open();
    const c = (await screen.findByRole("heading", { name: "caveman" })).closest("section")!;
    expect(within(c).getByText(/Önceki sürümde daha yüksek güvenle kaydedilmişti; kanıtı yetersiz olduğu için düşürüldü/)).toBeInTheDocument();
    expect(within(c).queryByText(/^Kimlik doğrulandı \(/)).not.toBeInTheDocument();
    const p = screen.getByRole("heading", { name: "ponytail" }).closest("section")!;
    expect(within(p).queryByText(/Kurulum kaydı katalogla uyuşuyor/)).not.toBeInTheDocument();
    await userEvent.click(within(p).getByText(/Yapılmayan kontroller/));
    expect(within(p).getByText(/yerel olarak düzenlenebilir/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/Kimlik doğrulandı: evet/);
  });


  it("keeps a local hash, a self-declared reference and an authenticated reference as three different things, and never goes green for a hash", async () => {
    const digest = "a".repeat(64);
    scanned([
      obs({ id: 1, subject: "gitleaks", ladder: ladder({ artifact: art({ fingerprint: "computed", sha256: digest, bytes: 10 }) }) }),
      obs({
        id: 2, subject: "strix",
        ladder: ladder({ artifact: art({ fingerprint: "computed", sha256: "b".repeat(64), reference: "unauthenticated", referenceKind: "executable", matchResult: "matches_unauthenticated" }) }),
      }),
      obs({
        id: 3, subject: "graphify", identityVerified: true, version: "0.0.1", versionSource: "yetkili referans özeti",
        ladder: ladder({
          identity: "validated", version: "known", versionTrust: "validated", artifactAuth: "digest_match",
          artifact: art({ fingerprint: "computed", sha256: "c".repeat(64), reference: "authenticated", referenceKind: "executable", matchResult: "matches_authenticated", referenceEpoch: "d".repeat(64) }),
        }),
      }),
    ]);
    await open();
    const lines = async (name: string) => within((await screen.findByRole("heading", { name })).closest("section")!).getByRole("list", { name: "Kanıt basamakları" });
    const local = await lines("gitleaks");
    expect(within(local).getByText(/Yerel parmak izi hesaplandı \(SHA-256 aaaaaaaaaaaa…; yalnızca baytları tanımlar, kaynağı kanıtlamaz\)/)).toBeInTheDocument();
    expect(within(local).getByText(/Doğrulanmış referans özeti yok/)).toBeInTheDocument();
    expect(within(local).getByText(/^Kimlik doğrulanmadı/)).toBeInTheDocument();
    const declared = await lines("strix");
    expect(within(declared).getByText(/Referans özeti var ama doğrulanmamış \(kendi beyanı; güven dayanağı değil\)/)).toBeInTheDocument();
    expect(within(declared).getByText(/doğrulanmamış bir referansla eşleşiyor \(kanıt değil\)/)).toBeInTheDocument();
    expect(within(declared).getByText(/^Kimlik doğrulanmadı/)).toBeInTheDocument();
    expect(within(declared).queryByText(/^Kimlik doğrulandı \(/)).not.toBeInTheDocument();
    const verified = await lines("graphify");
    expect(within(verified).getByText(/Referans özeti var ve bağımsız olarak doğrulanmış/)).toBeInTheDocument();
    expect(within(verified).getByText(/Kurulu dosya doğrulanmış referansla eşleşiyor/)).toBeInTheDocument();
    expect(within(verified).getByText(/Kimlik doğrulandı \(doğrulanmış referans \+ kurulu dosya eşleşmesi\)/)).toBeInTheDocument();
    // even a verified identity says nothing about whether the tool works
    expect(within(verified).getByText(/İşlev test edilmedi/)).toBeInTheDocument();
    expect(document.querySelector(".badge-passed")).toBeNull();
    expect(document.body.textContent).not.toMatch(/[A-Za-z]:\\Users|\/Users\//);
    expect(document.body.textContent).not.toContain(digest.repeat(2)); // only a short prefix is shown, never a path
  });

  it("shows mismatches, unknown fingerprints, invalidated verdicts, archive digests and record links honestly", async () => {
    scanned([
      obs({ id: 1, subject: "gitleaks", ladder: ladder({ identity: "conflicted", artifact: art({ fingerprint: "computed", sha256: "e".repeat(64), reference: "authenticated", referenceKind: "executable", matchResult: "mismatches_authenticated" }) }) }),
      obs({ id: 2, subject: "strix", ladder: ladder({ artifact: art({ fingerprint: "unknown", unknownReason: "redirected" }) }) }),
      obs({ id: 3, subject: "graphify", ladder: ladder({ reclassified: true, artifact: art({ fingerprint: "computed", sha256: "f".repeat(64), invalidated: true }) }) }),
      obs({ id: 4, subject: "markitdown", ladder: ladder({ artifact: art({ fingerprint: "computed", sha256: "1".repeat(64), reference: "authenticated", referenceKind: "archive", matchResult: "incomparable" }) }) }),
      obs({ id: 5, subject: "agent-browser", ladder: ladder({ installRecord: true, publisher: "self_declared", identity: "evidence_available", artifact: art({ recordLink: "unverifiable" }) }) }),
      obs({ id: 6, subject: "superpowers", method: "claude-plugin-declaration", ladder: ladder({ installRecord: true, identity: "record_corroborated", installedFilesSeen: true, publisher: "marketplace", artifact: art({ recordLink: "folder_by_name" }) }) }),
    ]);
    await open();
    const get = async (n: string) => within((await screen.findByRole("heading", { name: n })).closest("section")!);
    expect((await get("gitleaks")).getByText(/Kurulu dosya doğrulanmış referanstan FARKLI/)).toBeInTheDocument();
    expect((await get("gitleaks")).getByText(/Çelişkili gözlemler var/)).toBeInTheDocument();
    expect((await get("strix")).getByText(/Parmak izi hesaplanamadı \(bağlantı\/yönlendirme olduğu için okunmadı\); sonuç bilinmiyor/)).toBeInTheDocument();
    expect((await get("graphify")).getByText(/Önceki doğrulama geçersiz: ilke veya referans kümesi değişti/)).toBeInTheDocument();
    expect((await get("markitdown")).getByText(/Referans, açılan dosyanın türüne\/biçimine bağlanamıyor .*karşılaştırılamaz/)).toBeInTheDocument();
    expect((await get("agent-browser")).getByText(/Kayıt ile dosya arasındaki bağ doğrulanamadı/)).toBeInTheDocument();
    expect((await get("superpowers")).getByText(/klasör adıyla eşleşiyor \(dosya içeriğiyle değil\)/)).toBeInTheDocument();
    for (const n of ["gitleaks", "strix", "graphify", "markitdown", "agent-browser", "superpowers"]) {
      expect((await get(n)).queryByText(/^Kimlik doğrulandı \(/)).not.toBeInTheDocument();
    }
  });

  it("offers fingerprinting only after the PATH permission, explains that file content is read but nothing is run, and sends the choice", async () => {
    await open();
    const fp = await screen.findByRole("checkbox", { name: /parmak izini \(SHA-256\) hesapla/ });
    expect(fp).toBeDisabled();
    expect(fp).not.toBeChecked();
    expect(screen.getByText(/içeriği okunur/)).toBeInTheDocument();
    expect(screen.getByText(/dosya çalıştırılmaz/)).toBeInTheDocument();
    expect(screen.getByText(/kaynağını kanıtlamaz/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("checkbox", { name: /Bilinen program adlarını ara/ }));
    expect(fp).toBeEnabled();
    await userEvent.click(fp);
    await userEvent.click(screen.getByRole("button", { name: "Bu bilgisayarı tara" }));
    expect(calls("run_discovery_scan")[0].args).toEqual({ request: { pathExecutables: true, userConfig: false, project: null, fingerprintArtifacts: true } });
  });
});
