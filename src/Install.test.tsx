import { render, screen, within, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { createBackend, makeEntry, type Backend } from "./test/backend";
import { incompleteState, refused, toolState } from "./test/installMock";
import type { InstallResult } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const HELLO = "synthetic-hello";
const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);

const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};

/** The catalog with the synthetic tool in the given state. */
const openCatalog = async (status: Parameters<typeof toolState>[1] = "available", over: Parameters<typeof toolState>[2] = {}) => {
  backend.entries.push(makeEntry(HELLO, { displayName: "Sentetik Merhaba", origin: "user" }));
  backend.install.states.set(HELLO, toolState(HELLO, status, over));
  render(<App />);
  await go("Araç Kütüphanesi");
  return within(await screen.findByRole("group", { name: `Kurulum: ${HELLO}` }));
};

beforeEach(() => {
  backend = createBackend();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("status of every tool (nothing is installable in a release-like backend)", () => {
  it("shows 'verification incomplete' with its reason for all thirteen catalog entries and offers no installation", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const groups = await screen.findAllByRole("group", { name: /^Kurulum: / });
    expect(groups).toHaveLength(13);
    for (const g of groups) {
      expect(within(g).getByText("Doğrulama tamamlanmadı")).toBeInTheDocument();
      expect(within(g).getByText(/incelenmiş ve onaylanmış bir kurulum tarifi yok/)).toBeInTheDocument();
      expect(within(g).queryByRole("button", { name: /önizlemesini aç/ })).not.toBeInTheDocument();
    }
    // the old statements stay true and visible
    expect(screen.getByText(/Bu uygulama bu sürümde hiçbir şeyi kurmaz/)).toBeInTheDocument();
    expect(screen.getAllByText("Kurulu değil / yönetilmiyor")).toHaveLength(13);
    expect(calls("prepare_install")).toHaveLength(0);
  });

  it.each([
    ["verification_incomplete", "Doğrulama tamamlanmadı"],
    ["license_review_required", "Lisans incelemesi gerekli"],
    ["available", "Kurulabilir"],
    ["ready_for_approval", "Onayınız bekleniyor"],
    ["installing", "Kuruluyor…"],
    ["installed_verified", "Kurulu ve doğrulandı"],
    ["failed_needs_attention", "Başarısız / dikkat gerekiyor"],
    ["removed", "Kaldırıldı"],
  ] as const)("the status %s is told apart in words (not by colour)", async (status, label) => {
    const panel = await openCatalog(status);
    expect(panel.getByText(label)).toBeInTheDocument();
    // all eight labels are distinct
    expect(new Set(["Doğrulama tamamlanmadı", "Lisans incelemesi gerekli", "Kurulabilir", "Onayınız bekleniyor", "Kuruluyor…", "Kurulu ve doğrulandı", "Başarısız / dikkat gerekiyor", "Kaldırıldı"]).size).toBe(8);
  });

  it("a license-pending tool is not installable even though everything else is in order", async () => {
    const panel = await openCatalog("license_review_required", { canInstall: false, reasons: [{ code: "license_not_accepted", text: "Lisans, kabul edildi sonucuyla bir incelemeden geçmedi." }] });
    expect(panel.getByText(/Lisans, kabul edildi sonucuyla/)).toBeInTheDocument();
    expect(panel.queryByRole("button", { name: /önizlemesini aç/ })).not.toBeInTheDocument();
  });
});

describe("catalog -> preview -> confirm -> result -> installed details", () => {
  it("walks the whole workflow with real components and sends only identifiers and the acknowledgement", async () => {
    const panel = await openCatalog("available");
    expect(panel.getByText("Kurulabilir")).toBeInTheDocument();
    // the old disabled run button is replaced by an honest "Çalıştır" that cannot be used
    expect(screen.getByRole("button", { name: "Çalıştır" })).toBeDisabled();

    // 1. preview: nothing is changed by opening it
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    const preview = await screen.findByRole("heading", { name: /Kurulum önizlemesi: Sentetik synthetic-hello 1\.0\.0/ });
    expect(preview).toBeInTheDocument();
    expect(calls("confirm_install")).toHaveLength(0);
    const scope = within(preview.closest("section")!);
    for (const text of [
      /Windows x64/, /0123456789abcdef0123456789abcdef01234567/, /artifact\.bin/, new RegExp("ab".repeat(32)), /Kurulacak dosyanın kendisi \(arşiv değil\)/,
      /Ağ kullanılmayacak/, /SENTETİK DENEME/, /Üçüncü taraf bildirimleri/, /synthetic-runtime - GPL WITH exception/, /kimlik kanıtı DEĞİLDİR/, /tek kullanımlıktır/i,
    ]) {
      expect(scope.getAllByText(text).length).toBeGreaterThan(0);
    }
    const effects = scope.getByRole("table", { name: "Dosya sistemi etkileri" });
    expect(within(effects).getByText("versions/synthetic-hello/1.0.0/artifact.bin")).toBeInTheDocument();
    expect(within(effects).getAllByText("Hayır (geçici)").length).toBeGreaterThan(0);
    const never = scope.getByRole("list", { name: "Yapılmayacaklar" });
    expect(within(never).getByText(/Hiçbir program çalıştırılmaz/)).toBeInTheDocument();
    expect(within(never).getByText(/PATH/)).toBeInTheDocument();
    // no absolute path anywhere in the preview
    expect(scope.getByRole("table").textContent).not.toMatch(/[A-Za-z]:\\|\/Users\//);

    // 2. the confirm button needs the explicit acknowledgement
    const confirm = scope.getByRole("button", { name: "Onaylıyorum ve kur" });
    expect(confirm).toBeDisabled();
    await userEvent.click(scope.getByRole("checkbox", { name: /okudum ve anladım/ }));
    expect(confirm).toBeEnabled();
    await userEvent.click(confirm);

    // 3. result
    expect(await screen.findByRole("heading", { name: "Sonuç: Tamamlandı" })).toBeInTheDocument();
    expect(screen.getByText(/Kurulum tamamlandı ve doğrulandı\. Hiçbir program çalıştırılmadı\./)).toBeInTheDocument();
    expect(screen.getByText(`txn-${"a".repeat(32)}`)).toBeInTheDocument();

    // exactly the identifiers, the digest the person saw and the acknowledgement were sent -- nothing else
    const sent = calls("confirm_install")[0].args as { request: Record<string, unknown> };
    expect(Object.keys(sent.request).sort()).toEqual(["acknowledged", "offerId", "previewDigest"]);
    expect(sent.request.acknowledged).toBe(true);
    expect(JSON.stringify(calls("prepare_install")[0].args)).toBe(JSON.stringify({ request: { entryId: HELLO, action: "install" } }));
    for (const c of backend.calls) {
      expect(JSON.stringify(c.args ?? {})).not.toMatch(/"(path|url|bytes|sha256|command|script|destination)"/i);
    }

    // 4. installed-tool details
    await userEvent.click(screen.getByRole("button", { name: "Araç ayrıntılarını göster" }));
    const details = await screen.findByRole("heading", { name: /Kurulu araç ayrıntıları/ });
    const d = within(details.closest("section")!);
    expect(d.getAllByText("Kurulu ve doğrulandı").length).toBeGreaterThan(0);
    expect(d.getByText(`versions/${HELLO}/1.0.0/artifact.bin`)).toBeInTheDocument();
    expect(d.getAllByRole("listitem").filter((li) => /Geçti:/.test(li.textContent ?? "")).length).toBeGreaterThanOrEqual(5);
    expect(d.getByText("Bu uygulamanın yapmadıkları")).toBeInTheDocument();
  });

  it("closing the preview with 'Vazgeç' withdraws the offer and nothing happens", async () => {
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    expect(await within(await screen.findByRole("group", { name: `Kurulum: ${HELLO}` })).findByText("Onayınız bekleniyor")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Vazgeç" }));
    await waitFor(() => expect(calls("cancel_install")).toHaveLength(1));
    expect(Object.keys((calls("cancel_install")[0].args as { request: object }).request)).toEqual(["offerId"]);
    expect(calls("confirm_install")).toHaveLength(0);
    expect(await within(await screen.findByRole("group", { name: `Kurulum: ${HELLO}` })).findByText("Kurulabilir")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /Kurulum önizlemesi/ })).not.toBeInTheDocument();
  });

  it("leaving the page while a preview is open withdraws its offer", async () => {
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    await go("Ayarlar");
    await waitFor(() => expect(calls("cancel_install")).toHaveLength(1));
    expect(calls("confirm_install")).toHaveLength(0);
  });

  it("a refused confirmation (replay, stale, changed preview) is shown as refused and nothing is claimed", async () => {
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    backend.install.nextConfirm = refused("consent_already_used", "Bu onay zaten kullanıldı. Aynı onay ikinci kez kabul edilmez; önizlemeyi yeniden açın.");
    await userEvent.click(screen.getByRole("checkbox", { name: /okudum ve anladım/ }));
    await userEvent.click(screen.getByRole("button", { name: "Onaylıyorum ve kur" }));
    expect(await screen.findByRole("heading", { name: "Sonuç: Reddedildi — hiçbir şey yapılmadı" })).toBeInTheDocument();
    expect(screen.getByText(/Bu onay zaten kullanıldı/)).toBeInTheDocument();
    expect(screen.getByText("consent_already_used")).toBeInTheDocument();
    expect(screen.queryByText(/Kurulum tamamlandı/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Araç ayrıntılarını göster" })).not.toBeInTheDocument();
  });

  it("an expired preview disables the confirmation (Rust enforces it too)", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    backend.install.expiresInSeconds = 3;
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    await userEvent.click(screen.getByRole("checkbox", { name: /okudum ve anladım/ }));
    expect(screen.getByRole("button", { name: "Onaylıyorum ve kur" })).toBeEnabled();
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(await screen.findByText(/Önizlemenin süresi doldu/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Onaylıyorum ve kur" })).toBeDisabled();
  });

  it("a failed installation shows the rollback honestly, including what could NOT be undone", async () => {
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    const failed: InstallResult = {
      outcome: "failed", auditRecorded: true, action: "install", entryId: HELLO, version: "1.0.0", code: "fs_io", message: "Disk yazma hatası (örneğin yer kalmadı). İşlem geri alındı. Geri alma durumu: KISMEN geri alındı; bir kişinin bakması gerekiyor",
      receipt: null, rollback: { status: "partial", undone: ["hazırlık klasörü kaldırıldı"], notUndone: ["version folder: kayıtla eşleşmeyen bir dosya var; dokunulmadı"], notCovered: ["tool_user_data"] }, state: toolState(HELLO, "failed_needs_attention", { reasons: [{ code: "unfinished_transaction", text: "Yarım kalmış bir işlem var." }] }),
    };
    backend.install.nextConfirm = failed;
    await userEvent.click(screen.getByRole("checkbox", { name: /okudum ve anladım/ }));
    await userEvent.click(screen.getByRole("button", { name: "Onaylıyorum ve kur" }));
    expect(await screen.findByRole("heading", { name: "Sonuç: Başarısız — geri alındı" })).toBeInTheDocument();
    expect(screen.getByText("KISMEN geri alındı — bir kişinin bakması gerekiyor")).toBeInTheDocument();
    expect(screen.getByText(/Geri ALINAMAYANLAR/)).toBeInTheDocument();
    expect(screen.getByText(/kayıtla eşleşmeyen bir dosya var; dokunulmadı/)).toBeInTheDocument();
  });

  it("an installed tool can be removed through its own preview and confirmation, and shows as removed", async () => {
    const panel = await openCatalog("installed_verified");
    expect(panel.getByText("Kurulu ve doğrulandı")).toBeInTheDocument();
    expect(panel.queryByRole("button", { name: "Kurulum önizlemesini aç" })).not.toBeInTheDocument();
    await userEvent.click(panel.getByRole("button", { name: "Kaldırma önizlemesini aç" }));
    const heading = await screen.findByRole("heading", { name: /Kaldırma önizlemesi/ });
    const scope = within(heading.closest("section")!);
    expect(scope.getByText("Aracın kendi kullanıcı verisi: dokunulmaz.")).toBeInTheDocument();
    await userEvent.click(scope.getByRole("checkbox", { name: /okudum ve anladım/ }));
    await userEvent.click(scope.getByRole("button", { name: "Onaylıyorum ve kaldır" }));
    expect(await screen.findByText(/Kaldırma tamamlandı/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Tamam" }));
    expect(await within(await screen.findByRole("group", { name: `Kurulum: ${HELLO}` })).findByText("Kaldırıldı")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Kurulum önizlemesini aç" })).toBeInTheDocument();
  });

  it("an unfinished transaction is shown as needing attention and can be recovered only by an explicit action", async () => {
    const panel = await openCatalog("failed_needs_attention", { canInstall: false, canRemove: false, reasons: [{ code: "unfinished_transaction", text: "Yarım kalmış bir işlem var. Önce \"Kurtarmayı çalıştır\" ile durumu denetleyin." }] });
    expect(panel.getByText("Başarısız / dikkat gerekiyor")).toBeInTheDocument();
    expect(calls("recover_installations")).toHaveLength(0);
    await userEvent.click(panel.getByRole("button", { name: "Kurtarmayı çalıştır" }));
    expect(await screen.findByText(/1 işlem geri alındı, 0 işlem tamamlandı, 0 işlem hâlâ dikkat gerektiriyor/)).toBeInTheDocument();
    expect(await within(await screen.findByRole("group", { name: `Kurulum: ${HELLO}` })).findByText("Kurulabilir")).toBeInTheDocument();
  });

  it("a backend error while opening a preview is shown with its code and leaves everything unchanged", async () => {
    const panel = await openCatalog("available");
    backend.failures.prepare_install = { code: "INSTALL_NEEDS_ATTENTION", message: "Bu araç dikkat gerektiriyor; önce durumu denetleyin." };
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("INSTALL_NEEDS_ATTENTION");
    expect(screen.queryByRole("heading", { name: /Kurulum önizlemesi/ })).not.toBeInTheDocument();
  });
});

describe("the interface cannot ask for what the application never does", () => {
  it("has no run, download, path-picking or script control anywhere in the catalog", async () => {
    await openCatalog("installed_verified");
    for (const bad of [/^Çalıştır$/, /^İndir/, /Dosya seç/, /Gözat/, /Betik/, /URL/i]) {
      const hits = screen.queryAllByRole("button", { name: bad }).filter((b) => !(b as HTMLButtonElement).disabled);
      expect(hits).toHaveLength(0);
    }
    expect(screen.queryByRole("textbox", { name: /yol|path|komut|command/i })).not.toBeInTheDocument();
  });

  it("every status is still reachable by keyboard: the primary action is a real button", async () => {
    const panel = await openCatalog("available");
    const b = panel.getByRole("button", { name: "Kurulum önizlemesini aç" });
    b.focus();
    expect(b).toHaveFocus();
    expect(incompleteState("x").status).toBe("verification_incomplete");
  });
});

describe("audit honesty (Issue #22) and the local-use licence in the real components", () => {
  it("an operation that happened but could not be audited is shown as needing attention, never as a plain success, and can be reconciled", async () => {
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    const attention = toolState(HELLO, "failed_needs_attention", {
      installedVersion: "1.0.0", canInstall: false, canRemove: false,
      reasons: [{ code: "audit_outcome_missing", text: "Son işlem tamamlandı ve doğrulandı, ancak denetim kaydı yazılamadı." }],
    });
    backend.install.nextConfirm = {
      outcome: "needs_attention", auditRecorded: false, action: "install", entryId: HELLO, version: "1.0.0", code: "audit_outcome_not_persisted",
      message: "İşlem YAPILDI ve doğrulandı, ancak denetim kaydı yazılamadı. Gerçek durum ve makbuz doğrudur.",
      receipt: { txnId: `txn-${"b".repeat(32)}`, action: "install", version: "1.0.0", completedAt: "2027-01-01T12:00:00Z", artifactSha256: "ab".repeat(32), kept: [] },
      rollback: null, state: attention,
    };
    // (the real backend's state after the operation is what the panel reloads once the answer is in)
    backend.install.states.set(HELLO, attention);
    backend.install.auditReconciled = 1;
    await userEvent.click(screen.getByRole("checkbox", { name: /okudum ve anladım/ }));
    await userEvent.click(screen.getByRole("button", { name: "Onaylıyorum ve kur" }));
    expect(await screen.findByRole("heading", { name: "Sonuç: Yapıldı — ama dikkat gerekiyor" })).toBeInTheDocument();
    expect(screen.getByText(/İşlem YAPILDI ve doğrulandı, ancak denetim kaydı yazılamadı/)).toBeInTheDocument();
    expect(screen.getByText(/denetim kaydı yazılamadı; gerçek durum ve makbuz doğrudur/)).toBeInTheDocument();
    expect(screen.getByText("audit_outcome_not_persisted")).toBeInTheDocument();
    expect(screen.queryByText(/Kurulum tamamlandı ve doğrulandı/)).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sonuç: Tamamlandı" })).not.toBeInTheDocument();
    // the tool is shown as needing attention and only an explicit recovery action reconciles it
    await userEvent.click(screen.getByRole("button", { name: "Tamam" }));
    const group = within(await screen.findByRole("group", { name: `Kurulum: ${HELLO}` }));
    expect(await group.findByText("Başarısız / dikkat gerekiyor")).toBeInTheDocument();
    expect(calls("recover_installations")).toHaveLength(0);
    await userEvent.click(group.getByRole("button", { name: "Kurtarmayı çalıştır" }));
    expect(await screen.findByText(/1 eksik denetim kaydı makbuzdan yeniden kuruldu, 0 kayıt kurulamadı/)).toBeInTheDocument();
  });

  it("a refusal whose own audit row could not be written says so", async () => {
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    backend.install.nextConfirm = { ...refused("audit_unavailable", "Denetim kaydı yazılamadığı için hiçbir şey yapılmadı. Onay kullanıldı; önizlemeyi yeniden açın."), auditRecorded: false };
    await userEvent.click(screen.getByRole("checkbox", { name: /okudum ve anladım/ }));
    await userEvent.click(screen.getByRole("button", { name: "Onaylıyorum ve kur" }));
    expect(await screen.findByRole("heading", { name: "Sonuç: Reddedildi — hiçbir şey yapılmadı" })).toBeInTheDocument();
    expect(screen.getByText(/Denetim kaydı yazılamadığı için hiçbir şey yapılmadı/)).toBeInTheDocument();
    expect(screen.getByText(/denetim kaydı yazılamadı; gerçek durum ve makbuz doğrudur/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Araç ayrıntılarını göster" })).not.toBeInTheDocument();
  });

  it("the preview of a local-use licence says it is for this computer only and never for redistribution", async () => {
    backend.install.licenseState = "accepted_local_use";
    const panel = await openCatalog("available");
    await userEvent.click(panel.getByRole("button", { name: "Kurulum önizlemesini aç" }));
    const heading = await screen.findByRole("heading", { name: /Kurulum önizlemesi/ });
    const scope = within(heading.closest("section")!);
    expect(scope.getByText(/Yalnızca bu bilgisayardaki yerel kullanım için kabul edildi \(yeniden dağıtım veya paketleme için DEĞİL\)/)).toBeInTheDocument();
    expect(scope.getByText(/Kaynak, bağımsız bir kanıt kaydıyla depo, iş akışı ve commit.e bağlandı/)).toBeInTheDocument();
  });
});
