import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { ErrorBoundary } from "./components/common";
import { createBackend, makeEntry, pref, rec, report, type Backend } from "./test/backend";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const go = async (name: string) => {
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name }));
  return screen.findByRole("heading", { level: 1, name });
};
const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);

beforeEach(() => {
  backend = createBackend();
});

describe("navigation", () => {
  it("shows the eight sections and marks the current one", async () => {
    render(<App />);
    const nav = screen.getByRole("navigation", { name: "Ana gezinme" });
    for (const name of ["Ana Sayfa", "Projeler", "Araç Kütüphanesi", "Öneriler", "Bilgisayar Envanteri", "İstem Kütüphanesi", "Etkinlik ve Denetim", "Ayarlar"]) {
      expect(within(nav).getByRole("button", { name })).toBeInTheDocument();
    }
    expect(within(nav).getByRole("button", { name: "Ana Sayfa" })).toHaveAttribute("aria-current", "page");
    await go("Ayarlar");
    expect(within(nav).getByRole("button", { name: "Ayarlar" })).toHaveAttribute("aria-current", "page");
  });
});

describe("dashboard honesty", () => {
  it("does not claim health before any check ran and shows the new counts", async () => {
    render(<App />);
    expect(await screen.findByText(/Henüz sağlık kontrolü çalıştırılmadı/)).toBeInTheDocument();
    expect(screen.queryByText("Tüm kontroller başarılı")).not.toBeInTheDocument();
    expect(screen.getByText(/hiçbir aracın gerçekten çalıştığı doğrulanmış değildir/)).toBeInTheDocument();
    expect(screen.getByText("Katalogdaki kayıt").nextSibling).toHaveTextContent("13");
    expect(screen.getByText("Kurulu / yönetilen araç").nextSibling).toHaveTextContent("0");
    expect(screen.getByText(/Kayıtlı kararlar yalnızca isteğinizdir/)).toBeInTheDocument();
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

describe("tool library", () => {
  it("lists all thirteen entries with kinds, none installed, every run button disabled with the backend's reason", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    expect(await screen.findAllByRole("region")).toHaveLength(13);
    expect(screen.getAllByText("Kurulu değil / yönetilmiyor")).toHaveLength(13);
    const kindOf = (id: string) => within(screen.getByRole("heading", { name: id }).closest("section")!);
    expect(kindOf("anthropic-frontend-design").getByText("Ajan eklentisi")).toBeInTheDocument();
    expect(kindOf("strix").getByText("Çalıştırılabilir araç")).toBeInTheDocument();
    expect(kindOf("agent-browser").getByText("Çalıştırılabilir araç")).toBeInTheDocument();
    expect(kindOf("agentic-awesome-skills").getByText("Yetenek koleksiyonu")).toBeInTheDocument();
    const buttons = await screen.findAllByRole("button", { name: "Kur / çalıştır" });
    expect(buttons).toHaveLength(13);
    buttons.forEach((b) => expect(b).toBeDisabled());
    expect(calls("explain_execution_block")).toHaveLength(13);
    await waitFor(() => expect(screen.getAllByText(/Faz 1B'de hiçbir araç kurulamaz/).length).toBeGreaterThan(0));
    expect(screen.getByText(/Aktif güvenlik testi için kapsamlı yetki gerekir/)).toBeInTheDocument();
    expect(screen.queryByText(/^Doğrulandı$/)).not.toBeInTheDocument();
  });

  it("shows Strix as quarantined with its execution policy", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const strix = (await screen.findByRole("heading", { name: "strix" })).closest("section")!;
    expect(within(strix).getByText("Karantinada")).toBeInTheDocument();
    expect(within(strix).getByText(/Kapsamlı yazılı yetki olmadan çalıştırma yasak/)).toBeInTheDocument();
  });

  it("enabling or disabling an entry calls the typed command", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const card = (await screen.findByRole("heading", { name: "agent-browser" })).closest("section")!;
    await userEvent.click(within(card).getByRole("checkbox", { name: /Katalogda etkin/ }));
    expect(calls("set_catalog_entry_enabled")).toContainEqual({ cmd: "set_catalog_entry_enabled", args: { entryId: "agent-browser", enabled: false } });
    await waitFor(() => expect(within(card).getByRole("checkbox", { name: /Katalogda etkin/ })).not.toBeChecked());
  });

  it("removing needs a second confirmation, says nothing is uninstalled, and the entry can be restored", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const card = () => screen.getByRole("heading", { name: "graphify" }).closest("section")!;
    await screen.findByRole("heading", { name: "graphify" });
    await userEvent.click(within(card()).getByRole("button", { name: "Katalogdan kaldır" }));
    expect(calls("remove_catalog_entry")).toHaveLength(0);
    expect(within(card()).getByText(/hiçbir şey kaldırılmaz/)).toBeInTheDocument();
    await userEvent.click(within(card()).getByRole("button", { name: "Vazgeç" }));
    expect(calls("remove_catalog_entry")).toHaveLength(0);
    await userEvent.click(within(card()).getByRole("button", { name: "Katalogdan kaldır" }));
    await userEvent.click(within(card()).getByRole("button", { name: "Evet, kaydı kaldır" }));
    expect(calls("remove_catalog_entry")).toEqual([{ cmd: "remove_catalog_entry", args: { entryId: "graphify" } }]);
    expect(await screen.findByRole("heading", { name: /Katalogdan kaldırılanlar \(1\)/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Geri ekle (kapalı olarak)" }));
    expect(calls("restore_catalog_entry")).toHaveLength(1);
    await waitFor(() => expect(screen.queryByRole("heading", { name: /Katalogdan kaldırılanlar/ })).not.toBeInTheDocument());
  });

  it("adding a repository: invalid link is explained, nothing can be added before a valid preview and a chosen kind", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const url = await screen.findByLabelText("GitHub bağlantısı");
    const add = screen.getByRole("button", { name: "Aday olarak ekle" });
    expect(add).toBeDisabled();
    await userEvent.type(url, "https://evil.example/a/b");
    await userEvent.click(screen.getByRole("button", { name: "Bağlantıyı denetle" }));
    expect(await screen.findByText(/Yalnızca github.com adresi kabul edilir/)).toBeInTheDocument();
    expect(add).toBeDisabled();
    expect(calls("add_catalog_entry")).toHaveLength(0);
  });

  it("adding a repository: duplicates are flagged and not addable", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    await userEvent.type(await screen.findByLabelText("GitHub bağlantısı"), "https://github.com/x/gitleaks");
    await userEvent.selectOptions(screen.getByLabelText("Bu bir… (tür)"), "executable_tool");
    await userEvent.click(screen.getByRole("button", { name: "Bağlantıyı denetle" }));
    expect(await screen.findByText(/Bu depo zaten katalogda/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Aday olarak ekle" })).toBeDisabled();
  });

  it("adding a repository: a valid link plus a kind adds an offline, unreviewed, disabled candidate and says nothing was downloaded", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    const url = await screen.findByLabelText("GitHub bağlantısı");
    await userEvent.type(url, "https://github.com/acme/widget");
    await userEvent.click(screen.getByRole("button", { name: "Bağlantıyı denetle" }));
    expect(await screen.findByText(/Bağlantı geçerli/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Aday olarak ekle" })).toBeDisabled(); // kind not chosen yet
    await userEvent.selectOptions(screen.getByLabelText("Bu bir… (tür)"), "agent_plugin");
    await userEvent.type(screen.getByLabelText(/Ne işe yarıyor/), "Deneme eklentisi");
    await userEvent.click(screen.getByRole("button", { name: "Aday olarak ekle" }));
    expect(calls("add_catalog_entry")).toEqual([{ cmd: "add_catalog_entry", args: { url: "https://github.com/acme/widget", kind: "agent_plugin", purpose: "Deneme eklentisi" } }]);
    expect(await screen.findByText(/hiçbir şey indirilmedi veya çalıştırılmadı/i)).toBeInTheDocument();
    const card = (await screen.findByRole("heading", { name: "acme/widget" })).closest("section")!;
    expect(within(card).getByText("Sizin eklediğiniz aday")).toBeInTheDocument();
    expect(within(card).getByText(/Uzak veri çekilmedi/)).toBeInTheDocument();
    expect(within(card).getByRole("checkbox", { name: /Katalogda etkin/ })).not.toBeChecked();
    expect(within(card).getByRole("button", { name: "Kur / çalıştır" })).toBeDisabled();
  });

  it("hostile text from a repository purpose is rendered as plain text, not markup", async () => {
    backend.entries.push(makeEntry("evil", { origin: "user", purpose: "<img src=x onerror=alert(1)> <b>kalın</b>", displayName: "evil/evil", sourceKey: "evil/evil" }));
    render(<App />);
    await go("Araç Kütüphanesi");
    expect(await screen.findByText(/<img src=x onerror=alert\(1\)>/)).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
    expect(document.querySelector("section b")).toBeNull();
  });

  it("filters by kind", async () => {
    render(<App />);
    await go("Araç Kütüphanesi");
    await screen.findByRole("heading", { name: "strix" });
    await userEvent.selectOptions(screen.getByLabelText("Göster"), "skill_collection");
    expect(screen.queryByRole("heading", { name: "strix" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "agentic-awesome-skills" })).toBeInTheDocument();
  });
});

describe("projects and preferences", () => {
  it("labels sample projects, separates desired choice from actual state, and never shows an installed state", async () => {
    render(<App />);
    await go("Projeler");
    expect(await screen.findByText("ÖRNEK VERİ")).toBeInTheDocument();
    const card = (await screen.findByRole("heading", { name: /Örnek: Web sitesi/ })).closest("section")!;
    expect(within(card).getByText("örnek")).toBeInTheDocument();
    expect(within(card).getByText("Etkinleştirmek istiyorum")).toBeInTheDocument();
    const actual = within(card).getByText(/Kurulu değil \/ yönetilmiyor/);
    expect(actual).toHaveTextContent(/Doğrulanmadı/);
    expect(actual).toHaveTextContent(/Kapsam bilinmiyor/);
    expect(within(card).queryByText(/Doğrulandı|Kuruldu|Kurulu(?! değil)/)).not.toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /Proje kaydını kaldır/ })).not.toBeInTheDocument();
  });

  it("registers a real project only as a reference and says the folder is never touched", async () => {
    render(<App />);
    await go("Projeler");
    expect(await screen.findByText(/klasörünüzü açmaz, taramaz/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText(/Takma ad/), "benim-sitem");
    await userEvent.type(screen.getByLabelText("Görünen ad"), "Benim sitem");
    await userEvent.type(screen.getByLabelText(/Özel not/), "D:\\sitem");
    await userEvent.click(screen.getByRole("button", { name: "Proje kaydını ekle" }));
    expect(calls("add_project_reference")).toEqual([{ cmd: "add_project_reference", args: { alias: "benim-sitem", displayName: "Benim sitem", note: "D:\\sitem" } }]);
    const card = (await screen.findByRole("heading", { name: /Benim sitem/ })).closest("section")!;
    expect(within(card).getByText("gerçek proje kaydı")).toBeInTheDocument();
    expect(within(card).getByText(/uygulama bu klasöre hiç dokunmaz/)).toBeInTheDocument();
  });

  it("an invalid alias from the backend is shown, not swallowed", async () => {
    backend.failures.add_project_reference = { code: "PROJECT_ALIAS_INVALID", message: "Takma ad geçersiz." };
    render(<App />);
    await go("Projeler");
    await userEvent.type(await screen.findByLabelText(/Takma ad/), "C:\\x");
    await userEvent.type(screen.getByLabelText("Görünen ad"), "x");
    await userEvent.click(screen.getByRole("button", { name: "Proje kaydını ekle" }));
    expect(await screen.findByText(/PROJECT_ALIAS_INVALID/)).toBeInTheDocument();
  });

  it("removing a reference is two-step and only affects the app's record", async () => {
    backend.projects.push({ projectId: "p2", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [] });
    render(<App />);
    await go("Projeler");
    const card = () => screen.getByRole("heading", { name: /Benim sitem/ }).closest("section")!;
    await screen.findByRole("heading", { name: /Benim sitem/ });
    await userEvent.click(within(card()).getByRole("button", { name: "Proje kaydını kaldır" }));
    expect(within(card()).getByText(/klasörünüze hiçbir şey olmaz/)).toBeInTheDocument();
    expect(calls("remove_project_reference")).toHaveLength(0);
    await userEvent.click(within(card()).getByRole("button", { name: "Evet, kaldır" }));
    expect(calls("remove_project_reference")).toEqual([{ cmd: "remove_project_reference", args: { alias: "benim-sitem" } }]);
  });

  it("saves a decision with the full key and shows backend refusals", async () => {
    render(<App />);
    await go("Projeler");
    const card = (await screen.findByRole("heading", { name: /Örnek: Web sitesi/ })).closest("section")!;
    await userEvent.selectOptions(within(card).getByLabelText("Kayıt"), "agent-browser");
    await userEvent.selectOptions(within(card).getByLabelText("Karar"), "rejected");
    await userEvent.click(within(card).getByRole("button", { name: "Kararı kaydet" }));
    expect(calls("set_project_preference")).toContainEqual({
      cmd: "set_project_preference",
      args: { alias: "ornek-web-sitesi", entryId: "agent-browser", agentSurface: "claude-code-terminal", platformTarget: "windows-x64", decision: "rejected", remindDays: null },
    });
    backend.failures.set_project_preference = { code: "PREFERENCE_NOT_ACTIVATABLE", message: "Uyumluluk bilinmiyor." };
    await userEvent.selectOptions(within(card).getByLabelText("Karar"), "accepted");
    await userEvent.click(within(card).getByRole("button", { name: "Kararı kaydet" }));
    expect(await within(card).findByText(/PREFERENCE_NOT_ACTIVATABLE/)).toBeInTheDocument();
  });

  it("offers postponing with a bounded number of days and an explicit reset", async () => {
    render(<App />);
    await go("Projeler");
    const card = () => screen.getByRole("heading", { name: /Örnek: Web sitesi/ }).closest("section")!;
    await screen.findByRole("heading", { name: /Örnek: Web sitesi/ });
    await userEvent.selectOptions(within(card()).getByLabelText("Kayıt"), "caveman");
    await userEvent.selectOptions(within(card()).getByLabelText("Karar"), "postponed");
    const days = within(card()).getByLabelText("Kaç gün sonra hatırlat");
    expect(days).toHaveAttribute("min", "1");
    expect(days).toHaveAttribute("max", "365");
    await userEvent.click(within(card()).getByRole("button", { name: "Kararı sıfırla" }));
    expect(calls("reset_project_preference")).toEqual([
      { cmd: "reset_project_preference", args: { alias: "ornek-web-sitesi", entryId: "gitleaks", agentSurface: "local-cli", platformTarget: "windows-x64" } },
    ]);
  });

  it("flags a material change but treats a plain version bump as informational only", async () => {
    backend.projects[0].preferences = [
      pref({ entryId: "a", entryName: "Alfa", decision: "rejected", materialChange: true }),
      pref({ entryId: "b", entryName: "Beta", decision: "rejected", revisionChanged: true }),
    ];
    render(<App />);
    await go("Projeler");
    expect(await screen.findByText(/önemli ölçüde değişti/)).toBeInTheDocument();
    expect(screen.getByText(/bu tek başına yeniden sormaz/)).toBeInTheDocument();
  });
});

describe("recommendation preview", () => {
  it("is clearly a synthetic preview and fills the form from a labelled example", async () => {
    render(<App />);
    await go("Öneriler");
    expect(await screen.findByText(/öneri önizlemesidir/)).toBeInTheDocument();
    await screen.findByRole("option", { name: "React bileşeni tasarlamak" });
    await userEvent.selectOptions(screen.getByLabelText(/Örnek görev/), "react-component");
    expect(screen.getByLabelText("Görev metni")).toHaveValue("React bileşeni tasarla");
    expect(screen.getByLabelText(/İlgili dosya adları/)).toHaveValue("PricingTable.tsx");
  });

  it("sends the task, files, surface, platform and a hypothetical availability — never a path", async () => {
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Örnek: Web sitesi (örnek)" });
    await userEvent.selectOptions(screen.getByLabelText(/Örnek görev/), "backend-with-css-in-repo");
    await userEvent.selectOptions(screen.getByLabelText(/Frontend Design eklentisi zaten/), "global");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    const sent = calls("evaluate_task")[0].args!.input as Record<string, unknown>;
    expect(sent).toMatchObject({
      projectAlias: "ornek-web-sitesi", taskText: "SQL migration düzelt", evidenceFiles: ["styles.css"], agentSurface: "claude-code-terminal",
      platformTarget: "windows-x64", phase: "start", availability: [{ entryId: "anthropic-frontend-design", scope: "global", enabled: true }],
    });
    expect(String(sent.taskId)).toMatch(/^gorev-/);
  });

  it("says plainly when nothing is suggested and lists why other tools were not shown", async () => {
    backend.nextEvaluation = {
      taskId: "t", phase: "start", recommendations: [], disclaimer: "Hiçbir şey kurulmadı.",
      suppressed: [{ entryId: "anthropic-frontend-design", reasonCode: "no-ui-intent", reason: "Arayüz dosyaları var, ancak görevin kendisi arayüz işi gibi görünmüyor." }],
    };
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Örnek: Web sitesi (örnek)" });
    await userEvent.type(screen.getByLabelText("Görev metni"), "SQL düzelt");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    expect(await screen.findByText(/Bu görev için öneri yok/)).toBeInTheDocument();
    expect(screen.getByText(/Gösterilmeyenler ve nedenleri \(1\)/)).toBeInTheDocument();
    expect(screen.getByText("no-ui-intent")).toBeInTheDocument();
  });

  it("a suggestion can be enabled for the project, postponed or rejected; each records only a desired choice", async () => {
    backend.nextEvaluation = { taskId: "t", phase: "start", recommendations: [rec()], suppressed: [], disclaimer: "Hiçbir şey kurulmadı." };
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Örnek: Web sitesi (örnek)" });
    await userEvent.type(screen.getByLabelText("Görev metni"), "React bileşeni tasarla");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    expect(await screen.findByText("Öneri", { selector: ".badge" })).toBeInTheDocument();
    expect(screen.getByText(/Ne olur\?/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Bu proje için etkinleştir?" }));
    expect(calls("set_project_preference").at(-1)!.args).toMatchObject({ entryId: "anthropic-frontend-design", decision: "accepted", agentSurface: "claude-code-terminal" });
    expect(await screen.findByText(/Hiçbir şey kurulmadı\./, { selector: "p.ok-text" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Bu proje için etkinleştir?" })).not.toBeInTheDocument();
  });

  it("informational suggestions (unknown compatibility, Strix) have no enable button", async () => {
    backend.nextEvaluation = {
      taskId: "t", phase: "start", disclaimer: "x", suppressed: [],
      recommendations: [
        rec({ entryId: "strix", displayName: "Strix", level: "informational", canEnable: false, notes: ["Kapsamlı yazılı yetki olmadan çalıştırılamaz."] }),
        rec({ entryId: "agent-browser", displayName: "agent-browser", level: "informational", canEnable: false }),
      ],
    };
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Örnek: Web sitesi (örnek)" });
    await userEvent.type(screen.getByLabelText("Görev metni"), "pentest planla");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    expect(await screen.findAllByText("Yalnızca bilgi")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "Bu proje için etkinleştir?" })).not.toBeInTheDocument();
    expect(screen.getByText(/kapsamlı yazılı yetki olmadan çalıştırılamaz/)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Reddet / bir daha sorma" })).toHaveLength(2);
  });

  it("shows at most the candidates it is given, with license and overlap warnings", async () => {
    backend.nextEvaluation = {
      taskId: "t", phase: "start", disclaimer: "x", suppressed: [],
      recommendations: [
        rec({
          entryId: "agentic-awesome-skills", displayName: "agentic-awesome-skills",
          skillCandidates: [
            { skillId: "pdf-a", name: "PDF A", description: "pdf", sourceSha: "a".repeat(40), licenseSpdx: null, licenseWarning: true, conflictsWith: ["Superpowers"] },
          ],
        }),
      ],
    };
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Örnek: Web sitesi (örnek)" });
    await userEvent.type(screen.getByLabelText("Görev metni"), "skill bul");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    expect(await screen.findByText(/lisans bilinmiyor/, { selector: "span.bad-text" })).toBeInTheDocument();
    expect(screen.getByText(/şunlarla çakışabilir: Superpowers/)).toBeInTheDocument();
    expect(screen.getByText(/hiçbiri kurulmaz/)).toBeInTheDocument();
  });

  it("a backend refusal is shown, and a new task gets a new id", async () => {
    backend.failures.evaluate_task = { code: "PROJECT_NOT_FOUND", message: "Böyle bir proje kaydı yok." };
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Örnek: Web sitesi (örnek)" });
    await userEvent.type(screen.getByLabelText("Görev metni"), "x");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    expect(await screen.findByText(/PROJECT_NOT_FOUND/)).toBeInTheDocument();
    const first = screen.getByText(/Görev kimliği:/).textContent;
    await userEvent.click(screen.getByRole("button", { name: "Yeni görev başlat" }));
    expect(screen.getByText(/Görev kimliği:/).textContent).not.toBe(first);
  });
});

describe("prompt library", () => {
  it("lists three read-only planned sources and no usable search/download", async () => {
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
    expect(calls("run_health_checks")).toHaveLength(1);
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
    expect(calls("set_setting")).toContainEqual({ cmd: "set_setting", args: { key: "theme", value: "dark" } });
    await userEvent.click(screen.getByRole("radio", { name: "Sistem ayarını kullan" }));
    await waitFor(() => expect(document.documentElement).not.toHaveAttribute("data-theme"));
  });

  it("startup is off by default and only changes through the user's switch", async () => {
    render(<App />);
    await go("Ayarlar");
    const toggle = await screen.findByRole("checkbox", { name: /otomatik başlat/ });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    expect(calls("set_startup_enabled")).toHaveLength(0);
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(calls("set_startup_enabled")).toContainEqual({ cmd: "set_startup_enabled", args: { enabled: true } });
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
    await waitFor(() => expect(calls("report_ui_error").some((c) => String(c.args?.message).includes("kaboom"))).toBe(true));
  });
});

describe("closure: project-scoped tasks and immutable classification", () => {
  it("switching the project, agent or platform starts a new task and clears the previous result", async () => {
    backend.projects.push({ projectId: "p2", alias: "benim-sitem", displayName: "Benim sitem", kind: "reference", localNote: "", createdAt: "2026-10-17T00:00:00Z", preferences: [] });
    backend.nextEvaluation = { taskId: "t", phase: "start", recommendations: [rec()], suppressed: [], disclaimer: "Hiçbir şey kurulmadı." };
    render(<App />);
    await go("Öneriler");
    await screen.findByRole("option", { name: "Benim sitem" });
    await userEvent.type(screen.getByLabelText("Görev metni"), "React bileşeni tasarla");
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    expect(await screen.findByText("Öneri", { selector: ".badge" })).toBeInTheDocument();
    const firstId = String((calls("evaluate_task")[0].args!.input as Record<string, unknown>).taskId);

    await userEvent.selectOptions(screen.getByLabelText("Proje"), "benim-sitem");
    expect(screen.queryByText("Öneri", { selector: ".badge" })).not.toBeInTheDocument(); // nothing inherited from the other project
    expect(screen.getByText(/Görev kimliği:/)).not.toHaveTextContent(firstId);
    await userEvent.click(screen.getByRole("button", { name: "Değerlendir" }));
    const second = calls("evaluate_task")[1].args!.input as Record<string, unknown>;
    expect(second.projectAlias).toBe("benim-sitem");
    expect(second.taskId).not.toBe(firstId);

    const secondId = String(second.taskId);
    await userEvent.selectOptions(screen.getByLabelText("Ajan yüzeyi"), "antigravity-ide");
    expect(screen.getByText(/Görev kimliği:/)).not.toHaveTextContent(secondId);
    const thirdId = screen.getByText(/Görev kimliği:/).textContent!;
    await userEvent.selectOptions(screen.getByLabelText("İşletim sistemi"), "macos-arm64");
    expect(screen.getByText(/Görev kimliği:/).textContent).not.toBe(thirdId);
  });

  it("a removed link comes back with its old type; the type is locked and explained", async () => {
    backend.entries.push(makeEntry("acme-widget", { origin: "user", kind: "skill_collection", displayName: "acme/widget", sourceKey: "acme/widget", removedAt: "2026-10-17T10:00:00Z", enabled: false }));
    render(<App />);
    await go("Araç Kütüphanesi");
    await userEvent.type(await screen.findByLabelText("GitHub bağlantısı"), "https://github.com/acme/widget");
    await userEvent.selectOptions(screen.getByLabelText("Bu bir… (tür)"), "agent_plugin"); // the user first picks another type
    await userEvent.click(screen.getByRole("button", { name: "Bağlantıyı denetle" }));
    const kind = (await screen.findByLabelText("Bu bir… (tür)")) as HTMLSelectElement;
    await waitFor(() => expect(kind).toBeDisabled());
    expect(kind.value).toBe("skill_collection");
    expect(screen.getByRole("note")).toHaveTextContent(/tür geri eklerken değiştirilemez/);
    await userEvent.click(screen.getByRole("button", { name: "Aday olarak ekle" }));
    expect(calls("add_catalog_entry")).toEqual([{ cmd: "add_catalog_entry", args: { url: "https://github.com/acme/widget", kind: "skill_collection", purpose: "" } }]);
  });

  it("a backend refusal to change the type is shown, not swallowed", async () => {
    backend.failures.add_catalog_entry = { code: "KIND_CHANGE_NOT_ALLOWED", message: "Tür geri eklerken değiştirilemez." };
    render(<App />);
    await go("Araç Kütüphanesi");
    await userEvent.type(await screen.findByLabelText("GitHub bağlantısı"), "https://github.com/acme/other");
    await userEvent.click(screen.getByRole("button", { name: "Bağlantıyı denetle" }));
    await screen.findByText(/Bağlantı geçerli/);
    await userEvent.selectOptions(screen.getByLabelText("Bu bir… (tür)"), "executable_tool");
    await userEvent.click(screen.getByRole("button", { name: "Aday olarak ekle" }));
    expect(await screen.findByText(/KIND_CHANGE_NOT_ALLOWED/)).toBeInTheDocument();
  });
});
