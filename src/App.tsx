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
import { RecommendationsPage } from "./pages/Recommendations";
import { applyTheme, isThemeChoice } from "./theme";

const PAGES = [
  { id: "dashboard", label: "Ana Sayfa" },
  { id: "projects", label: "Projeler" },
  { id: "tools", label: "Araç Kütüphanesi" },
  { id: "recommendations", label: "Öneriler" },
  { id: "inventory", label: "Bilgisayar Envanteri" },
  { id: "prompts", label: "İstem Kütüphanesi" },
  { id: "activity", label: "Etkinlik ve Denetim" },
  { id: "settings", label: "Ayarlar" },
] as const;

type PageId = (typeof PAGES)[number]["id"];

export default function App() {
  const [page, setPage] = useState<PageId>("dashboard");
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
  const navigate = (id: string) => setPage((PAGES.find((p) => p.id === id)?.id ?? "dashboard") as PageId);

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
                onClick={() => setPage(p.id)}
              >
                {p.label}
              </button>
            </li>
          ))}
        </ul>
        <p className="phase small">Faz 2B · salt okunur envanter ve pasif kanıt (kurulum yok)</p>
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
          {page === "projects" && <ProjectsPage />}
          {page === "tools" && <CatalogPage />}
          {page === "recommendations" && <RecommendationsPage />}
          {page === "inventory" && <InventoryPage />}
          {page === "prompts" && <PromptsPage />}
          {page === "activity" && <ActivityPage />}
          {page === "settings" && <SettingsPage />}
        </ErrorBoundary>
      </main>
    </div>
  );
}
