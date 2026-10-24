import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import { ErrorBoundary, ErrorNotice, useLoad } from "./components/common";
import { ActivityPage } from "./pages/Activity";
import { DashboardPage } from "./pages/Dashboard";
import { ProjectsPage } from "./pages/Projects";
import { PromptsPage } from "./pages/Prompts";
import { SettingsPage } from "./pages/Settings";
import { CatalogPage } from "./pages/Catalog";
import { InventoryPage } from "./pages/Inventory";
import { PlanPage } from "./pages/Plan";
import { RecommendationsPage } from "./pages/Recommendations";
import { applyTheme, isThemeChoice } from "./theme";
import type { MatrixHandoff, TargetRequest } from "./workspace";

const PAGES = [
  { id: "dashboard", label: "Ana Sayfa" },
  { id: "projects", label: "Projeler" },
  { id: "tools", label: "Araç Kütüphanesi" },
  { id: "recommendations", label: "Öneriler" },
  { id: "inventory", label: "Bilgisayar Envanteri" },
  { id: "plan", label: "Kurulum Planı" },
  { id: "prompts", label: "İstem Kütüphanesi" },
  { id: "activity", label: "Etkinlik ve Denetim" },
  { id: "settings", label: "Ayarlar" },
] as const;

type PageId = (typeof PAGES)[number]["id"];

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
  // A target (and optionally one suggested tool) another page asks the Projects page to open (project, operating system, agent surface). It only selects and points at what the matrix shows; any
  // normal navigation clears it, so it can never be applied twice or leak into an unrelated visit.
  const [handoff, setHandoff] = useState<MatrixHandoff | null>(null);
  const handoffs = useRef(0);
  const info = useLoad(api.getAppInfo);
  const headingRef = useRef<HTMLHeadingElement>(null);

  // Apply the saved theme as soon as it is known.
  useEffect(() => {
    api.getSettings().then((s) => applyTheme(isThemeChoice(s.theme) ? s.theme : "system")).catch(() => undefined);
  }, []);

  // Move focus to the page heading on navigation (keyboard / screen-reader users).
  useEffect(() => {
    headingRef.current?.focus();
  }, [page]);

  const current = PAGES.find((p) => p.id === page)!;
  const navigate = (id: string) => {
    setHandoff(null);
    setPage((PAGES.find((p) => p.id === id)?.id ?? "dashboard") as PageId);
  };
  const openInMatrix = (request: TargetRequest & { entryId?: string }) => {
    setHandoff({ alias: request.alias, platform: request.platform, surface: request.surface, entryId: request.entryId ?? null, nonce: ++handoffs.current });
    setPage("projects");
  };

  return (
    <div className="shell">
      <a className="skip" href="#main">Ana içeriğe geç</a>
      <nav aria-label="Ana gezinme" className="sidebar">
        <div className="brand">AI Toolchain<br />Manager</div>
        <ul>
          {PAGES.map((p) => (
            <li key={p.id}>
              <button
                type="button" className="nav-item" aria-current={p.id === page ? "page" : undefined}
                onClick={() => navigate(p.id)}
              >
                {p.label}
              </button>
            </li>
          ))}
        </ul>
        <p className="phase small">Faz 3D-2 · kurulum önizlemesi, açık onay ve yalnızca SENTETİK deneme araçlarının kurulumu; gerçek bir araç kurulamaz ve hiçbir program çalıştırılmaz</p>
      </nav>
      <main id="main" className="content">
        {info.data?.degraded && (
          <ErrorNotice
            error={info.data.startupError ?? { code: "DEGRADED", message: "Uygulama sınırlı (bozulmuş) modda çalışıyor; değişiklik yapılamaz." }}
          />
        )}
        <h1 tabIndex={-1} ref={headingRef}>{current.label}</h1>
        <ErrorBoundary key={page}>
          {page === "dashboard" && <DashboardPage onNavigate={navigate} />}
          {page === "projects" && <ProjectsPage handoff={handoff} onOpenInMatrix={openInMatrix} />}
          {page === "tools" && <CatalogPage onOpenInMatrix={openInMatrix} />}
          {page === "recommendations" && <RecommendationsPage onOpenInMatrix={openInMatrix} />}
          {page === "inventory" && <InventoryPage />}
          {page === "plan" && <PlanPage />}
          {page === "prompts" && <PromptsPage />}
          {page === "activity" && <ActivityPage />}
          {page === "settings" && <SettingsPage />}
        </ErrorBoundary>
      </main>
    </div>
  );
}
