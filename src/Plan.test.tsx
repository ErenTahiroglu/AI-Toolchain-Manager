import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { createBackend, makePlan, type Backend } from "./test/backend";
import type { InstallPlan } from "./types";

let backend: Backend;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => backend.handle(cmd, args),
}));

const calls = (cmd: string) => backend.calls.filter((c) => c.cmd === cmd);
const open = async () => {
  render(<App />);
  await userEvent.click(within(screen.getByRole("navigation")).getByRole("button", { name: "Kurulum Planı" }));
  await screen.findByRole("heading", { level: 1, name: "Kurulum Planı" });
};
const choose = async (entry = "gitleaks") => {
  await userEvent.selectOptions(await screen.findByLabelText("Araç"), entry);
};
const show = async () => {
  await userEvent.click(screen.getByRole("button", { name: /Planı göster/ }));
  await screen.findByRole("heading", { name: /için kurulum planı \(ön izleme\)/ });
};

beforeEach(() => {
  backend = createBackend();
});

describe("installation plan preview", () => {
  it("opens without asking the backend for any plan and says up front that nothing is installed", async () => {
    await open();
    expect(calls("plan_install")).toHaveLength(0);
    expect(screen.getByText(/Bu sayfa hiçbir şey kurmaz, indirmez, çalıştırmaz veya değiştirmez/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Planı göster/ })).toBeDisabled();
  });

  it("sends exactly the chosen question and shows the blocked plan with every reason", async () => {
    await open();
    await choose("gitleaks");
    await userEvent.selectOptions(screen.getByLabelText("Hedef bilgisayar türü"), "macos-arm64");
    await userEvent.selectOptions(screen.getByLabelText("Hedef ajan yüzeyi"), "claude-code-terminal");
    await userEvent.selectOptions(screen.getByLabelText("Kapsam"), "project");
    // a project plan cannot be requested without choosing which project
    expect(screen.getByRole("button", { name: /Planı göster/ })).toBeDisabled();
    await userEvent.selectOptions(await screen.findByLabelText("Proje"), "ornek-web-sitesi");
    await show();
    expect(calls("plan_install")[0].args).toEqual({
      request: { entryId: "gitleaks", platformTarget: "macos-arm64", agentSurface: "claude-code-terminal", scope: "project", methodId: null, projectAlias: "ornek-web-sitesi" },
    });
    expect(screen.getByText(/Bu plan uygulanamaz/)).toBeInTheDocument();
    const reasons = screen.getByRole("list", { name: "Engelleme nedenleri" });
    expect(within(reasons).getByText(/onaylanmış, tipli bir kurulum tarifi yok/)).toBeInTheDocument();
    expect(within(reasons).getByText(/bağımsız olarak incelenmedi/)).toBeInTheDocument();
    expect(screen.getByText(/Geri alma durumu:/)).toBeInTheDocument();
    expect(screen.getAllByText(/Geri alma YOK/).length).toBeGreaterThan(0);
  });

  it("presents steps as proposals that will not be applied, with what is missing, the network and the targets", async () => {
    await open();
    await choose();
    await show();
    expect(screen.getByRole("heading", { name: /Önerilen adımlar — UYGULANMAYACAK/ })).toBeInTheDocument();
    const steps = screen.getByRole("list", { name: "Önerilen adımlar" });
    expect(within(steps).getByText("Sürüm dosyasını indir")).toBeInTheDocument();
    expect(within(steps).getByText(/Eksik bilgi \(bu yüzden tipli işlem yazılamıyor\): beklenen SHA-256 özeti/)).toBeInTheDocument();
    expect(within(steps).getAllByText(/indirilecekti|karşılaştırılacaktı/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Bu önizleme ağa bağlanmadı/)).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Çakışmalar" })).toHaveTextContent("zaten bulunduğunu");
    expect(screen.getByRole("list", { name: "Uyarılar" })).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Hedef" })).toHaveTextContent("yayıncı beyanı, test edilmedi");
  });

  it("never offers an install, run, apply or approve control and never reads as a finished installation", async () => {
    await open();
    await choose();
    await show();
    const labels = screen.getAllByRole("button").map((b) => b.textContent ?? "");
    for (const l of labels) expect(l).not.toMatch(/^(Kur|Kurulumu başlat|Uygula|Onayla|Çalıştır|İndir)\b/i);
    expect(document.body.textContent).not.toMatch(/Kurulum tamamlandı|başarıyla kuruldu|Kuruldu\b|Onaylandı/);
    expect(document.body.textContent).not.toMatch(/[A-Za-z]:\\Users|\/Users\//);
    expect(document.querySelector(".badge-passed")).toBeNull();
  });

  it("states prerequisites with their source, and shows publisher text only as an unverified claim", async () => {
    await open();
    await choose();
    await show();
    const prereq = screen.getByRole("list", { name: "Ön koşullar" });
    expect(within(prereq).getByText("Bilinmiyor")).toBeInTheDocument();
    expect(within(prereq).getByText(/son tarama kaydından; güncel ve uygulanabilir kanıt yok — bilinmiyor, “yok” demek değil/)).toBeInTheDocument();
    expect(screen.getByText(/kaynak sabitleme: katalogda commit bildirilmemiş/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /Yayıncının bildirdiği bağımlılıklar \(doğrulanmadı\)/ })).toBeInTheDocument();
  });

  it("removes a stale plan when the question changes, and lets the user pick a method", async () => {
    await open();
    await choose();
    await show();
    await userEvent.selectOptions(screen.getByLabelText("Kurulum yöntemi"), "skills-cli");
    await waitFor(() => expect(calls("plan_install").length).toBe(2));
    expect(calls("plan_install")[1].args).toEqual({
      request: expect.objectContaining({ entryId: "gitleaks", methodId: "skills-cli" }),
    });
    await userEvent.selectOptions(screen.getByLabelText("Hedef bilgisayar türü"), "macos-arm64");
    expect(screen.queryByRole("heading", { name: /için kurulum planı/ })).not.toBeInTheDocument();
  });

  it("shows a refused request as an error and no plan", async () => {
    backend.failures["plan_install"] = { code: "PLAN_REQUEST_INVALID", message: "Plan isteği geçersiz." };
    await open();
    await choose();
    await userEvent.click(screen.getByRole("button", { name: /Planı göster/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("PLAN_REQUEST_INVALID");
    expect(screen.queryByRole("heading", { name: /için kurulum planı/ })).not.toBeInTheDocument();
  });

  it("a plan for a platform other than this computer says so", async () => {
    const r = { entryId: "gitleaks", platformTarget: "macos-arm64", agentSurface: "local-cli", scope: "user", methodId: null, projectAlias: null };
    const plan = makePlan(r);
    expect(plan.target.thisComputerMatches).toBe(false);
    await open();
    await choose();
    await userEvent.selectOptions(screen.getByLabelText("Hedef bilgisayar türü"), "macos-arm64");
    await show();
    expect(screen.getByText(/bu bilgisayarınkinden FARKLI/)).toBeInTheDocument();
  });

  describe("recipe, approval and rollback status", () => {
    const withRecipe = (recipe: Partial<InstallPlan["recipe"]>, over: Partial<InstallPlan> = {}) => {
      backend.planOverride = (r) => makePlan(r, { recipe: { ...makePlan(r).recipe, ...recipe }, ...over });
    };

    it("says that no reviewed recipe exists and that the plan is a draft without any approval", async () => {
      await open();
      await choose();
      await show();
      const status = screen.getByRole("list", { name: "Tarif ve onay durumu" });
      expect(within(status).getByText(/İncelenmiş bir tarif yok/)).toBeInTheDocument();
      expect(within(status).getByText(/Bu plan bir TASLAKTIR; kullanıcı onayı yok/)).toBeInTheDocument();
      expect(within(status).getByText(/Geri alma YOK/)).toBeInTheDocument();
      expect(screen.queryByText(/SENTETİK DEMO/)).not.toBeInTheDocument();
    });

    it("marks a plan built from a synthetic recipe as a demonstration, keeps review apart from approval and never calls rollback verified", async () => {
      withRecipe(
        { state: "validated", recipeId: "synthetic-tool-1.0.0", revision: 1, recipeDigest: "ab".repeat(32), synthetic: true, rollbackState: "simulated", authentication: "authenticated_reference" },
        {
          rollback: { available: false, reliability: "simulated", text: "Geri alma sözleşmesi yalnızca SENTETİK olarak benzetildi." },
          eligibility: { status: "blocked", canExecute: false, blockers: [{ code: "user_approval_missing", text: "Tarif incelenmiş olsa da bu plan için kullanıcı onayı yok." }], warnings: [] },
        },
      );
      await open();
      await choose();
      await show();
      expect(screen.getByText(/SENTETİK DEMO/)).toBeInTheDocument();
      expect(screen.getByText(/Gerçek bir kurulum değildir/)).toBeInTheDocument();
      const status = screen.getByRole("list", { name: "Tarif ve onay durumu" });
      expect(within(status).getByText(/Tarifin incelenmesi bir ONAY değildir/)).toBeInTheDocument();
      expect(within(status).getByText(/yalnızca SENTETİK test dizinlerinde benzetildi \(gerçek bir araçta DOĞRULANMADI\)/)).toBeInTheDocument();
      expect(within(status).getByText(/Bu plan bir TASLAKTIR; kullanıcı onayı yok/)).toBeInTheDocument();
      expect(screen.getByText(/Bu plan uygulanamaz/)).toBeInTheDocument();
      expect(within(screen.getByRole("list", { name: "Engelleme nedenleri" })).getByText(/kullanıcı onayı yok/)).toBeInTheDocument();
      expect(document.body.textContent).not.toMatch(/Geri alma (doğrulandı|çalışıyor|hazır)/);
      for (const b of screen.getAllByRole("button")) expect(b.textContent ?? "").not.toMatch(/^(Kur|Onayla|Uygula|Güncelle|Kaldır|Başlat)(\s|$)/);
    });

    it("explains in plain Turkish why a recipe was not accepted", async () => {
      withRecipe({ state: "invalid", recipeId: "x", revision: 1, recipeDigest: "cd".repeat(32), problems: ["artifact_not_authenticated", "changed_after_review", "digest_scope_not_exact_file"] });
      await open();
      await choose();
      await show();
      const status = screen.getByRole("list", { name: "Tarif ve onay durumu" });
      expect(within(status).getByText(/Bir tarif var ama GEÇERSİZ/)).toBeInTheDocument();
      expect(within(status).getByText(/bağımsız olarak doğrulanmış bir referansla eşleşmiyor/)).toBeInTheDocument();
      expect(within(status).getByText(/tarif incelemeden sonra değişti/)).toBeInTheDocument();
      expect(within(status).getByText(/özet bir arşive ait/)).toBeInTheDocument();
    });

    it("tells apart a designed rollback from a simulated one", async () => {
      withRecipe({ state: "validated", recipeId: "x", revision: 1, recipeDigest: "ef".repeat(32), rollbackState: "designed" }, { rollback: { available: false, reliability: "designed", text: "tasarım" } });
      await open();
      await choose();
      await show();
      expect(screen.getAllByText(/Geri alma yalnızca TASARLANDI \(hiçbir yerde denenmedi\)/).length).toBeGreaterThan(0);
    });
  });

});
